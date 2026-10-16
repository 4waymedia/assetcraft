#!/usr/bin/env python3
"""Draft a cue map (docs/track-analysis.md §5) for an audio file: the bridge until the Rust
analyser (phases A1–A4) writes the same format.

Dev tool only, never shipped. Needs Python 3.10+, numpy, librosa 0.11 (ISC), ffprobe and ffmpeg.
Times are source sample frames. Output: <audio>.cuemap.json next to the audio file.

    python tools/cuemap.py <audio>... [--credits copyright-claims.md]

What it finds: tempo and beats (librosa's DP beat tracker), the bar phase (bass + onset strength on
every 4th beat), sections (novelty of 4-bar windows of chroma + MFCC, peaks on 4-bar phrase starts),
section energy (mean RMS, normalised 0..1 across the track), hits (bass jumps of ≥ 6 dB at a bar line
= "drop"; the strongest downbeat onsets = "hit"), and Fit to length for 30/60/90/120 s (spec §6.5:
fade ending on the best bar line near the target; edit to the song's final 4–8 bars).
"""
import argparse
import json
import os
import re
import subprocess
import sys
import warnings

import librosa
import numpy as np

warnings.filterwarnings("ignore")
ANALYZER = "tools/cuemap.py 0.1 (librosa " + librosa.__version__ + ")"
SR = 22050
HOP = 512
TARGETS = (30, 60, 90, 120)


def probe(path):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "a:0", "-show_entries",
         "stream=sample_rate:format=duration", "-of", "json", path],
        capture_output=True, text=True, check=True).stdout
    j = json.loads(out)
    return int(j["streams"][0]["sample_rate"]), float(j["format"]["duration"])


def credits_for(path, credits_file):
    """The verbatim credit block for this file from copyright-claims.md (one section per file)."""
    if not credits_file or not os.path.exists(credits_file):
        return None
    text = open(credits_file, encoding="utf-8").read()
    name = os.path.basename(path)
    m = re.search(r"^### " + re.escape(name) + r"\n(.*?)(?=^### |\Z)", text, re.S | re.M)
    if not m:
        return None
    sec = m.group(1)
    get = lambda k: (re.search(r"- \*\*" + k + r":\*\* (.+)", sec) or [None, None])[1]
    block = re.search(r"```text\n(.*?)\n```", sec, re.S)
    artist = get("Artist") or ""
    return {
        "title": get("Title") or "",
        "artist": re.sub(r"\s*\(.*\)$", "", artist),
        "license": get("Licence") or "unknown",
        "credit": block.group(1) if block else "",
    }


def analyse(path, credits_file):
    native_sr, duration = probe(path)
    y, sr = librosa.load(path, sr=SR, mono=True)
    f2s = lambda t: int(round(t * native_sr))           # seconds → source frames
    oenv = librosa.onset.onset_strength(y=y, sr=sr, hop_length=HOP)
    tempo, bf = librosa.beat.beat_track(onset_envelope=oenv, sr=sr, hop_length=HOP)
    tempo = float(np.atleast_1d(tempo)[0])
    bt = librosa.frames_to_time(bf, sr=sr, hop_length=HOP)
    ac = librosa.autocorrelate(oenv - oenv.mean())
    ac /= ac[0] if ac[0] else 1
    lag = int(round(60 / tempo * sr / HOP)) if tempo > 0 else 1
    clarity = float(ac[lag]) if 0 < lag < len(ac) else 0.0

    S = np.abs(librosa.stft(y, n_fft=2048, hop_length=HOP))
    freqs = librosa.fft_frequencies(sr=sr, n_fft=2048)
    bass = S[freqs < 150].sum(axis=0)
    bass_db = 20 * np.log10(bass + 1e-9)
    bass_on = np.maximum(0, np.diff(bass, prepend=bass[0]))
    phase = int(np.argmax([bass_on[bf[p::4]].mean() + oenv[bf[p::4]].mean() for p in range(4)]))
    bars = bt[phase::4]
    rms_db = 20 * np.log10(librosa.feature.rms(S=S)[0] + 1e-9)
    at = lambda c, t: c[min(len(c) - 1, max(0, int(t * sr / HOP)))]

    chroma = librosa.feature.chroma_stft(S=S ** 2, sr=sr)
    mf = librosa.feature.mfcc(S=librosa.power_to_db(librosa.feature.melspectrogram(S=S ** 2, sr=sr)), n_mfcc=13)
    feat = np.vstack([librosa.util.normalize(chroma, axis=0), librosa.util.normalize(mf, axis=1)])
    bfr = librosa.time_to_frames(bars, sr=sr, hop_length=HOP)
    barfeat = librosa.util.sync(feat, bfr, aggregate=np.mean)
    barfeat = barfeat / (np.linalg.norm(barfeat, axis=0, keepdims=True) + 1e-9)
    nb = barfeat.shape[1]
    nov = np.zeros(nb)
    for i in range(4, nb - 4):
        a, b = barfeat[:, i - 4:i].mean(1), barfeat[:, i:i + 4].mean(1)
        nov[i] = 1 - float(a @ b / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-9))

    # Sections: novelty peaks on 4-bar phrase starts, at least 8 bars apart, about one per 16 bars.
    want = max(2, min(10, round(len(bars) / 16)))
    cand = sorted((i for i in range(4, min(nb, len(bars)) - 4, 4)), key=lambda i: -nov[i])
    bounds = []
    for i in cand:
        if all(abs(i - j) >= 8 for j in bounds) and nov[i] > 0.01:
            bounds.append(i)
        if len(bounds) >= want - 1:
            break
    bounds = sorted(bounds)
    edges = [0.0] + [float(bars[i]) for i in bounds] + [duration]
    energies = [float(np.mean(rms_db[int(a * sr / HOP):max(int(a * sr / HOP) + 1, int(b * sr / HOP))]))
                for a, b in zip(edges, edges[1:])]
    lo, hi = min(energies), max(energies)
    norm = [(e - lo) / (hi - lo) if hi > lo else 0.5 for e in energies]
    labels = []
    for k, e in enumerate(norm):
        if k == 0 and e < 0.6:
            labels.append("Intro")
        elif k == len(norm) - 1 and len(norm) > 1:
            labels.append("Outro")
        elif e >= 0.75:
            labels.append("Chorus")
        elif k + 1 < len(norm) and norm[k + 1] > e + 0.2:
            labels.append("Build")
        else:
            labels.append("Verse")
    sections = [{"start": f2s(a), "end": f2s(b), "label": l, "energy": round(e, 3)}
                for a, b, l, e in zip(edges, edges[1:], labels, norm)]

    # Hits: a sustained bass rise of ≥ 6 dB (2 bars after vs 2 bars before a bar line) at a local
    # peak, at least 8 bars apart = "drop"; the strongest downbeat onsets = "hit".
    hits = []
    def level(a, b):
        i, j = int(a * sr / HOP), max(int(a * sr / HOP) + 1, int(b * sr / HOP))
        return float(np.mean(bass_db[i:j]))
    rise = [level(bars[i], bars[i + 2]) - level(bars[i - 2], bars[i]) if 2 <= i < len(bars) - 2 else -99.0
            for i in range(len(bars))]
    for i in sorted(range(len(bars)), key=lambda i: -rise[i]):
        if rise[i] < 6:
            break
        if rise[i] >= max(rise[max(0, i - 2):i + 3]) and all(abs(i - j) >= 8 for j in [h["bar"] for h in hits]):
            hits.append({"bar": i, "frame": f2s(bars[i]), "kind": "drop", "strength": round(float(min(1.0, rise[i] / 12)), 3)})
    for h in hits:
        h.pop("bar")
    strengths = np.array([at(oenv, t) for t in bars]) if len(bars) else np.array([])
    if len(strengths):
        thr = np.percentile(strengths, 95)
        for t, s in zip(bars, strengths):
            if s >= thr and all(abs(f2s(t) - h["frame"]) > native_sr // 4 for h in hits):
                hits.append({"frame": f2s(t), "kind": "hit", "strength": round(float(s / (strengths.max() + 1e-9)), 3)})
    hits.sort(key=lambda h: h["frame"])

    bar_len = 60 / tempo * 4 if tempo > 0 else 2.0
    fits = {}
    for target in TARGETS:
        if duration < target + 5:
            continue
        fits[str(target)] = fit_for(target, duration, bars, nb, nov, barfeat, oenv, rms_db, at, bar_len, tempo, f2s)

    meta = credits_for(path, credits_file) or {}
    return {
        "version": 1,
        "analyzer": ANALYZER,
        "track": {
            "file": os.path.basename(path),
            "title": meta.get("title") or os.path.splitext(os.path.basename(path))[0],
            "artist": meta.get("artist", ""),
            "license": meta.get("license", "unknown"),
            "credit": meta.get("credit", ""),
        },
        "sampleRate": native_sr,
        "durationFrames": f2s(duration),
        "bpm": round(tempo, 2),
        "beatsPerBar": 4,
        "pulseClarity": round(clarity, 3),
        "beats": [f2s(t) for t in bt],
        "downbeats": [f2s(t) for t in bars],
        "sections": sections,
        "hits": hits,
        "fits": fits,
    }


def fit_for(target, dur, bars, nb, nov, barfeat, oenv, rms_db, at, bar_len, tempo, f2s):
    """Fit to length (spec §6.5): the best fade ending, and an edit to the song's ending."""
    best = None
    for k, t in enumerate(bars):
        if not (target - 6 <= t <= target + 1.5):
            continue
        phrase = 1.2 if k % 8 == 0 else 0.7 if k % 4 == 0 else 0.0
        sect = nov[k] if k < nb else 0.0
        hit = at(oenv, t) / (oenv.max() + 1e-9)
        score = phrase + 2.0 * sect + 0.6 * hit - 0.05 * abs(t - target) - 0.02 * max(0.0, at(rms_db, t) - np.median(rms_db))
        if best is None or score > best[0]:
            best = (score, k, float(t))
    fade = None
    if best:
        end = best[2]
        flen = min(2 * bar_len, 5.0)
        fade = {"mode": "fade", "lengthFrames": f2s(end), "segments": [[0, f2s(end)]],
                "crossfadeFrames": 0, "fadeOut": {"start": f2s(end - flen), "frames": f2s(flen)},
                "reasons": [f"fade ends on bar {best[1] + 1}" + (" (8-bar phrase)" if best[1] % 8 == 0 else "")],
                "changes": f"Cut to {int(round(end) // 60)}:{int(round(end) % 60):02d} with a fade out"}
    edit = None
    if nb > 8:
        lo, hi = max(1, nb - 8), max(1, nb - 4)
        tail = max(range(lo, hi + 1), key=lambda i: nov[i] + (0.2 if i % 4 == 0 else 0))
        if tail < len(bars):
            end_t = float(bars[tail])
            tail_len = dur - end_t
            body = target - tail_len
            cand = None
            for i, t in enumerate(bars):
                if i < 1 or i >= nb or abs(t - body) > 0.5 * bar_len:
                    continue
                sim = float(barfeat[:, i - 1] @ barfeat[:, tail - 1])
                s = sim + (0.3 if i % 4 == 0 else 0) - 0.03 * abs(t + tail_len - target)
                if cand is None or s > cand[0]:
                    cand = (s, i, float(t), sim)
            if cand and tail_len < target - 10:
                _, i, t, sim = cand
                xf = 60 / tempo if tempo > 0 else 0.5
                fmt = lambda s: (lambda r: f"{int(r // 60)}:{r % 60:04.1f}")(round(s, 1))
                edit = {"mode": "edit", "lengthFrames": f2s(t + tail_len), "segments": [[0, f2s(t)], [f2s(end_t), f2s(dur)]],
                        "crossfadeFrames": f2s(xf), "fadeOut": None, "similarity": round(sim, 3),
                        "reasons": [f"splice on bar {i + 1} (similarity {sim:.2f})", f"song's own ending: last {nb - tail} bars"],
                        "changes": f"Edited to {fmt(t + tail_len)}: 0:00–{fmt(t)} and {fmt(end_t)}–end, crossfaded"}
    chosen = edit if edit and edit.get("similarity", 0) >= 0.6 else fade
    return {"chosen": chosen["mode"] if chosen else None, "fade": fade, "edit": edit}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("audio", nargs="+")
    ap.add_argument("--credits", default=None)
    a = ap.parse_args()
    for path in a.audio:
        doc = analyse(path, a.credits)
        out = os.path.splitext(path)[0] + ".cuemap.json"
        with open(out + ".tmp", "w", encoding="utf-8", newline="\n") as f:
            json.dump(doc, f, indent=1, default=lambda o: o.item() if hasattr(o, "item") else str(o))
        os.replace(out + ".tmp", out)
        print(f"{os.path.basename(out)}: {doc['bpm']} BPM, {len(doc['downbeats'])} bars, "
              f"{len(doc['sections'])} sections, {len(doc['hits'])} hits, fits {sorted(doc['fits'], key=int)}")


if __name__ == "__main__":
    sys.exit(main())
