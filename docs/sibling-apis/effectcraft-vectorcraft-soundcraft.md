# Crafting Apps integration surfaces for AssetCraft: EffectCraft, VectorCraft, SoundCraft

Read-only source inventory at latest release tags (2026-10-15). Paths are relative to each app's repository root.

## Summary

| App | Version | Commit | Engine commands | UI-layer commands | Distinct total | MCP tools | Resources / templates / prompts |
| --- | --- | --- | --- | --- | --- | --- | --- |
| EffectCraft | v0.10.0 | `05955c0` | 731 | 111 | 842 | 33 | 2 / 0 / 0 |
| VectorCraft | v0.11.0 | `0a33cff` | 743 | 285 (2 overlap) | 1026 | 32 | 3 / 4 / 5 |
| SoundCraft | v0.7.0 | `3a28a65` | 478 | 66 | 544 | 16 | 2 / 0 / 0 |

### The 5 most important findings

1. **VectorCraft makes SVG/AI/EPS/PDF thumbnails without opening a document.** The engine command `file.place.info {path, thumbnail: 256}` returns `thumbnailBase64`, a PNG up to 512 px (`crates/engine/src/cmd/place/mod.rs:65`). `document.pdfInfo {thumbnail, thumbnailSize}` does the same for PDF and `.ai` pages (`cmd/fileio/mod.rs:158`).
   - `vectorcraft-format::preview()` reads the ≤256 px preview embedded in a `.vectorcraft` file without parsing the document (`crates/format/src/lib.rs:581`). That preview is only written when the saver opts in (`preview?: false`, `cmd/fileio/save.rs:714`).
   - These are reachable through headless MCP, the control channel, or by embedding `vectorcraft-engine` / `vectorcraft-mcp::Headless` in AssetCraft. None of those crates depend on UI code.
2. **EffectCraft poster frames work headless today.** Run `effectcraft-cli render-frame --project F.ecproj --comp C --time T --max-side 256 --out p.png --json` (`apps/effectcraft-cli/src/main.rs:570-603`). The in-process equivalent is `host::session()` plus `Session::render_rgba8(comp, comp.poster_time, 256)` (`crates/engine/src/lib.rs:1041`).
   - `.ectemplate` files are ZIPs that already contain a `poster.png` (≤320 px) and a `manifest.json` (`commands/essential.rs:10-13`, `:847-868`).
   - Watch out: the CLI renders at the playhead (CTI), not the comp's `poster_time`, and opens the demo project if `--project` is missing (`main.rs:29`).
3. **SoundCraft has a clean probe and waveform library, but no rendered waveform.** `soundcraft-audio-io` depends only on symphonia and does no filesystem I/O (`crates/audio-io/src/lib.rs:1-14`).
   - `probe(bytes, ext)` returns sample rate, channels, frames (duration) and BWF metadata (`lib.rs:96-121`, `:210`).
   - `peaks::Peaks::build` and `spans` give per-column min/max for a waveform (`peaks.rs:53`, `:130`).
   - No layer below the UI draws a waveform PNG. That gap needs a `soundcraft-cli waveform` subcommand (owner: SoundCraft).
4. **None of the three hands a file to an already-running instance on Windows or Linux.** Opening in a running app works only if it was started with `--control <port>`: EffectCraft `engine.execute file.open`, VectorCraft `app.open`, SoundCraft `engine.execute session.open`.
   - None of the control channels has authentication.
   - The EffectCraft and VectorCraft MSIs register no file associations, and their Linux MIME files are empty. Only SoundCraft's installer registers `.scraft` plus audio/MIDI "Open with".
   - Suggested fix (owner: each app): a single-instance forwarder plus file associations.
   - Port collision: VectorCraft's MCP auto-connects to `127.0.0.1:7979` (`crates/mcp/src/lib.rs:33`), and SoundCraft's `app` command also defaults to 7979 (`apps/soundcraft-cli/src/main.rs:362`). AssetCraft should assign explicit ports, or each app could publish its port in a port file.
5. **SoundCraft is a Crafting App only by its own claim.** Its README lists itself, with a link to `getartcraft.com/apps/soundcraft` (`README.md:35`, `:320-335`). The EffectCraft and VectorCraft READMEs leave it out of their Crafting Apps tables (`effectcraft/README.md:326-332`, `vectorcraft/README.md:303-309`), as does EffectCraft's in-app sibling list (`crates/engine/src/links.rs:13-20`). I did not check the website.
   - On FilmCraft interchange: EffectCraft sends comps to FilmCraft through `file.exportTimeline` (FCP XML, FCPXML, OTIO, EDL, AAF, OMF), using FilmCraft's own interchange crate and pre-rendering what FilmCraft can't draw (`commands/timeline.rs:219`, `crates/interchange/src/lib.rs:11-26`). There is no Dynamic Link-style live connection. VectorCraft and SoundCraft have no FilmCraft interchange; SoundCraft lists AAF/OMF as an alpha blocker.

---

## EffectCraft (v0.10.0, 05955c08b4fd025f1a09882df72e6dbff1a8148a)

### 0. What it is

EffectCraft is a clean-room, pure-Rust reimplementation of After Effects (`README.md:13`).

- **Formats:**
  - Project: `.ecproj` (versioned JSON) and an `.ecprojx` XML copy (`README.md:77`, `apps/effectcraft/src/main.rs:50-55`).
  - Lottie in and out (`README.md:157-160`).
  - Render Queue output to H.264, ProRes, HEVC, AV1, WebM, image sequences, GIF, WAV and AIFF, with no FFmpeg (`README.md:148-155`).
  - It cannot open `.aep` files (`README.md:227`).
- **License:** MIT OR Apache-2.0 (`Cargo.toml:9`, `README.md:360`).
- **Platforms:** macOS, Windows (x64/x86/ARM64), Linux, FreeBSD and Web (`README.md:255-316`).
- **Layers** (`docs/architecture.md:18-50`; crates below L5 may not depend on egui, eframe, winit, rfd, cpal or muda, `:12`):

| Layer | Crates |
| --- | --- |
| L0 | time, geom, color, vp9enc, opusenc, hevcenc, av1enc |
| L1 | raster, keyframe, segment, psd, path |
| L2 | project, text, effects, svg, pdf, model, track |
| L3 | render, media, expr, export, gpu, interchange, lottie, plugin |
| L4 | engine, script, host |
| L5 | ui-egui, automation |
| L6 | apps effectcraft, effectcraft-cli, effectcraft-web |

### 1. Desktop launch

- **Usage:** `effectcraft [--control <port>] [--demo|--empty|--home] [--] [project.ecproj | media…]` (`main.rs:3`). The parser is at `main.rs:78-115`.
  - An unknown option exits with code 1 (`main.rs:99`).
  - The first project file is opened with `file.open` (`main.rs:233`). All other files go through `file.import {paths}` (`main.rs:241`).
- **Control port:** `--control` takes 1–65535; 0 is rejected (`main.rs:117-124`). The fallback environment variable is `EFFECTCRAFT_CONTROL_PORT` (`main.rs:107`).
  - Started with `--control`, the window doesn't take focus (`main.rs:169`) and crash recovery is skipped (`main.rs:230`).
- **Other environment variables:** `EFFECTCRAFT_CONFIG_DIR` (`crates/host/src/app_dirs.rs:5`) and `WGPU_BACKEND`.
- **Single instance:** none. On macOS only, Finder/Dock opens reach the running app as Apple events (`apple_events.rs:1-10`).
- **Windows installer:** `effectcraft-<ver>-windows-<arch>.msi`, per-machine (`packaging/windows/effectcraft.wxs:12-16`).
  - Install folder is `INSTALLFOLDER`, defaulting to "EffectCraft" under Program Files and overridable (`:39-64`).
  - Binaries: `effectcraft.exe` (`:77`) and `effectcraft-cli.exe` (`:90`). An App Paths key is registered (`:83`).
  - No file associations (no `ProgId`/`Extension`). The Linux `mime.xml` is empty and the `.desktop` file has no `MimeType` (`.desktop:6`).

### 2. CLI (`apps/effectcraft-cli/src/main.rs:3-38`, dispatch `:441-640`)

| Subcommand | Key arguments | Lines |
| --- | --- | --- |
| `info` | — | `:443` |
| `commands` / `exec --list` | `[--filter] [--enabled] [--schemas]` | `:485-486`, `:790` |
| `exec <id>` | `[--params JSON]` | `:488` |
| `run <id> <json> …` | — | `:499-527` |
| `props` / `get` / `set` | `<comp> <layer> [<path> <value>] [--time]` | `:528-555` |
| `render-frame` (alias `frame`) | `[--comp] [--time\|--frame] [--max-side\|--scale] [--out] [--transparent]` | `:570-603` |
| `render` | `--out F [--format …]` and codec options | `:13-18`, `:655` |
| `render F.ecproj --queue` | — | `:21` |
| `render` / `render-frame --template T.ectemplate` | `[--values V.json]` | `:19-20`, `template.rs:23` |
| `bench` | — | `:811` |
| `script FILE.jsx \| --eval` | — | `:385` |
| `mcp` | `[--autosave \| --bridge PORT] [--automation-*-root]` | `:604-633` |

- **Global options:**
  - `--project F` (or a positional `.ecproj`), `--demo`, `--empty`. The default is the demo project, except for `mcp`, which starts empty (`:29`).
  - `--save` / `--save-as`, `--gpu`.
  - `--bridge PORT` makes any one-shot subcommand drive a running GUI (`:33`).
  - `--json` gives compact output, with errors as `{"error": …}` (`:34`).
- **Script files:** `.jsonl` command scripts run through `exec file.runScript '{"path":…}'` (`commands/file_more.rs:1014`).
- **Exit codes:** 0 = OK, 1 = error, 2 = usage (`main.rs:274-301`).

### 3. Command registry

- `CommandSpec` is defined at `crates/engine/src/commands/mod.rs:104-114`.
- Macros: `cmd!` (`:119`) and `query!` (`:139`), plus the wrappers `fe!` (`frontend.rs:11`), `res!` (`view.rs:166`), `fast!` and `chan!` (`viewer_cmds.rs:179-191`).
- `command_specs()` assembles about 80 modules (`:155-240`). A code comment says "~2,000 ids" (`:259`), but the static parse finds 731.
- 111 UI-only commands live in `crates/ui-egui/src/menus.rs:29-142`. They are reachable only through the control channel's menu dispatcher (`control.rs:171-181`).

**Engine commands (731, grouped by namespace):**
- `layer.` (121): addItem, addMarker, addMask, addShapeItem, addTextAnimator, addTextAnimatorProperty, addTextSelector, align, alignVideoToData, applyTextPreset, arrange, autoOrient, autoTrace, cameraSettings, castsShadows, centerAnchor, channels, create, deleteAllMarkers, distribute, enablePerChar3D, enableTimeRemap, environment, expressions, extendSelectionNext, extendSelectionPrevious, frameBlending, freezeFrame, freezeOnLastFrame, hideOtherVideo, lightSettings, markersLock, mask.featherFalloff, mask.hideLocked, mask.invert, mask.lock, mask.lockOthers, mask.mode, mask.motionBlur, mask.remove, mask.removeAll, mask.reset, mask.selectNext, mask.selectPrevious, mask.set, mask.shape, mask.unlockAll, moveEarlier, moveEarlier10, moveInToStart, moveLater, moveLater10, moveOutToEnd, new3dPrimitive, newAdjustment, newCamera, newContentAwareFill, newLight, newModel, newNull, newPrimitive, newShape, newSolid, newText, nudge, openLayer, openSource, precompose, quality, quickOffset, rename, revealExpressionErrors, revealInFinder, revealSource, sampling, sceneEditDetection, select, selectNext, selectPrevious, sequence, setBlendMode, setComment, setMask, setParent, setSwitch, setText, setTrackMatte, setTransform, settings, showAllVideo, slip, slipBack, slipForward, stretchInToTime, stretchOutToTime, style.add, style.bevelEmboss, style.colorOverlay, style.convertToEditable, style.dropShadow, style.globalLight, style.gradientOverlay, style.innerGlow, style.innerShadow, style.list, style.options, style.outerGlow, style.remove, style.removeAll, style.satin, style.showAll, style.stroke, style.toggle, timeReverse, timeStretch, timing, trackMatte, transform, tree, unlockAll, updateMarkersFromSource
- `view.` (78): 3d.activeCamera, 3d.back, 3d.bottom, 3d.custom1-3, 3d.default, 3d.front, 3d.last, 3d.left, 3d.right, 3d.top, addGuide, addPercentGuide, assign3dShortcut, channel, channel.{alpha,blue,blueColorized,green,greenColorized,red,redColorized,rgbStraight}, clearGuides, closeLockedViewer, convertGuides, customRgb, displayColor, displayColorManagement, exportGuides, exposure, extendedViewer, fastPreviewMode, fastPreviews.{adaptive,draft,fastDraft,off,wireframe}, fullScreen, get3D, grid, guides, importGuides, layerControls, layout, lockGuides, lookAtAll, lookAtSelected, moveGuide, newViewer, options, panelBackground, removeGuide, res.{auto,custom,full,half,quarter,third}, reset3DView, resetExposure, rulers, set3DView, set3DViewCamera, setPaneView, setRegionOfInterest, shareViewOptions, showSnapshot, simulateOutput, snapToGrid, snapToGuides, snapping, snappingOptions, splitLockedViewer, takeSnapshot, zoomIn, zoomOut
- `file.` (61): applyInterpretation, autoSave, clearRecent, clearRecentFootage, close, closeProject, collectFiles, consolidateFootage, createProxy, cycleBitDepth, executeFile, exportLottie, exportTimeline, findMissing, import, importLottie, importMultiple, importPlaceholder, importProject, importRecent, importSolid, importTimeline, importVanishingPoint, incrementAndSave, installScript, installScriptUIPanel, interpretFootage, interpretProxy, newCompFromSelection, newFromTemplate, newProject, open, openDemoProject, openRecent, projectSettings, recoveryInfo, reduceProject, relinkFootage, reloadFootage, rememberInterpretation, removeUnusedFootage, replaceFootage, replaceWithLayeredComp, replaceWithPlaceholder, replaceWithPrecomp, replaceWithSolid, revealInFinder, revert, runScript, save, saveAs, saveCopy, saveCopyAsXml, scripts.list, setProxy, setProxyNone, timelineFormats, uninstallScript, useProxy, watchFolder, watchFolder.poll
- `keys.` (36): audioToKeyframes, copy, delete, deselectAll, easyEase, easyEaseIn, easyEaseOut, exponentialScale, info, interpolation, move, nudge, nudgeBackward, nudgeBackward10, nudgeForward, nudgeForward10, paste, quickOffset, rpfCameraImport, select, selectAll, selectEqual, selectFollowing, selectLabelGroup, selectPrevious, set, setEase, setLabel, setSpatialTangents, smooth, timeReverse, toggleHold, toggleTransform, transform, velocity, wiggle
- `comp.` (31): activateRecent, addMarker, close, copyFrame, cropToLayerBounds, cropToRegionOfInterest, deletePreset, flowchart, info, miniFlowchart, new, open, openInEssentialGraphics, presets, renderer, responsiveTime, revealInProject, saveFrameAs, saveFrameAsExr, saveFrameAsPsd, savePreset, setPosterTime, setSwitch, settings, trimToWorkArea, vr.createEnvironment, vr.environments, vr.extractCubemap, vr.setView, workArea, workAreaToSelection
- `edit.` (27): clear, copy, copyExpressionOnly, copyWithPropertyLinks, copyWithRelativePropertyLinks, cut, deselectAll, duplicate, editOriginal, extractWorkArea, history, history.goto, history.list, label, liftWorkArea, paste, pasteReversedKeyframes, pasteSvg, pasteTextFormattingOnly, pasteTextMatchFormatting, purge, purgeUndo, redo, selectAll, selectLabelGroup, splitLayer, undo
- `essential.` (22): addComment, addGroup, addMedia, addMirror, addProperty, canAdd, exportTemplate, importTemplate, instance, linkProperty, list, move, pushToComp, remove, rename, revert, set, setName, setPrimary, soloSupported, templateInfo, unlinkProperty
- `track.` (22): analyze, apply, camera, delete, editTargetDialog, extractFaceMeasurements, mask, maskMethod, motion, new, options, optionsDialog, property, reset, select, setPoint, setTarget, setType, stabilize, status, stop, warpStabilizer
- `renderQueue.` (20): add, applyTemplate, deleteTemplate, duplicate, formats, list, move, remove, render, saveTemplate, setLog, setNotify, setOutput, setOutputModule, setOverflowFolders, setRender, setRenderSettings, setTemplateDefault, stop, templates
- `camera.` (19): analyze, cancel, create, createFromSolve, deletePoints, dolly, fromModel, fromView, linkFocusToLayer, linkFocusToPoi, orbit, orbitNull, pan, points, selectPoints, setFocusToLayer, setGroundPlane, solveStatus, stereoRig
- `effect.` (19): apply, applyLast, copy, duplicate, editDropdown, list, manage, paste, pickColor, plugins.list, plugins.load, remove, removeAll, reorder, reset, setLabel, swapTintColors, toggle, warning
- `prop.` (18): addKey, convertExpressionToKeyframes, deselect, duplicateGroup, editGradient, get, moveGroup, pickWhip, removeGroup, renameGroup, reset, select, separateDimensions, set, setExpression, setGroupEnabled, toggleAnimation, toggleKey
- `mask.` (17): addVertex, convertVertex, deleteVertices, featherPoint.{add,list,remove,set}, insertVertex, interpolate, interpolationOptions, moveVertices, new, remove, removeAll, selectVertices, setClosed, setVertex
- `time.` (17): back10, end, forward10, go, layerIn, layerOut, nextFrame, nextInOut, nextKey, previousFrame, previousInOut, previousKey, set, start, step, workAreaEnd, workAreaStart
- `roto.` (14): cancel, clearStrokes, freeze, model.download, model.install, model.remove, model.select, models, options, propagate, span, status, stroke, unfreeze
- `help.` (13): appPage, discord, docs, enableLogging, github, inAppTutorials, onlineTutorials, reportIssue, revealLogFile, sibling, systemInfo, systemReport, website
- `project.` (13): delete, deleteWithoutConfirmation, duplicate, move, newFolder, rename, select, setComment, setLabel, setProjectComment, summary, usage, usedIn
- `text.` (13): addSelector, animatorFontAxes, animatorProperties, delete, edit, endEdit, fontFeatures, fonts, insert, moveCaret, presets, removeAllAnimators, setSelection
- `app.` (12): about, capabilities, commandPalette, find, gpuInfo, hide, hideOthers, keyboardShortcuts, quit, settings, showAll, templates
- `footage.` (12): check, clearInOut, info, missing, open, overlayEdit, relink, reloadChanged, rippleInsertEdit, setIn, setOut, setTime
- `puppet.` (10): addPin, follow, info, mesh, movePin, recordOptions, recordPin, removePin, selectPins, setPin
- `anim.` (9): addKeyframe, applyBuiltinPreset, applyPreset, applyRecentPreset, browsePresets, builtinPresets, clearRecentPresets, reveal, savePreset
- `window.` (9): assignWorkspaceShortcut, editWorkspaces, panel, resetWorkspace, restoreScriptPanels, saveWorkspace, saveWorkspaceAs, scriptPanel, workspace
- `shape.` (8): dashes.add, dashes.remove, fillStroke, newPath, newShape, stroke.taper, stroke.wave, toolOptions
- `mediaBrowser.` (7): action, addFavorite, fileInfo, go, import, list, removeFavorite
- `shortcuts.` (7): conflicts, export, import, list, preset, reset, set
- `paint.` (6): brushPreset, options, presets, removeStroke, setCloneSource, stroke
- `path.` (6): convertToBezier, freeTransform, groupShapes, rotoBezier, setFirstVertex, ungroupShapes
- `playback.` (6): audio, cacheWhenIdle, scrubAudio, settings.get, settings.set, toggle
- `scriptui.` (6): click, close, get, list, mouse, set
- `face.` (5): model.download, model.install, model.remove, model.select, models
- `learn.` (5): list, start, state, step, stop
- `markers.` (5): convert, delete, list, nested, set
- `objectMatte.` (5): apply, freeze, propagate, status, unfreeze
- `prefs.` (5): get, open, pages, reset, set
- `templates.` (5): create, delete, list, saveAs, thumbnail
- `material.` (4): duplicateAssign, reset, revealSource, set
- `render.` (4): addOutputModule, backend, preRender, saveCurrentPreview
- `script.` (4): run, runTasks, settings.get, settings.save
- `jobs.` (3): cancel, list, wait
- `light.` (3): controlWithCamera, environmentBackground, fromModel
- `paths.` (3): nullsFollowPoints, pointsFollowNulls, tracePath
- `storage.` (3): clear, info, persist
- `warp.` (3): analyze, cancel, status
- `command.` (2): describe, list
- `contentFill.` (2): generate, set
- `expr.` (2): errors, languageMenu
- `liquify.` (2): clear, stroke
- Single-command namespaces: `cache.diskStats`, `editor.state`, `effects.copyPresetsFolderPath`, `engine.batch`, `item.metadata`, `motion.sketch`, `scopes.analyze`

**UI-only commands (111):**
- `timeline.` (53): collapseAll, column, graphEditor, keyAt.{anchor,opacity,position,rotation,scale}, moveInToTime, moveOutToTime, parentColumn, reveal.{anchor,effects,expressions,feather,levels,maskOpacity,maskPath,masks,material,missingEffects,opacity,paint,position,rotation,scale,selected,timeRemap,waveform}, revealAdd.{anchor,animated,effects,feather,levels,masks,opacity,position,rotation,scale}, scrollToLayer, scrollToTime, search, sourceName, switchesModes, trimInToTime, trimOutToTime, twirlSelected, workAreaBegin, workAreaEnd, zoomFit, zoomFrameToggle, zoomIn, zoomOut
- `tool.` (15): brush, camera, dolly, hand, orbit, panBehind, panCamera, pen, puppet, rotate, rotoBrush, selection, shape, type, zoom
- `view.` (12): actualSize, fastPreviews, fit, fitUpTo100, nextItem, previousItem, safeMargins, theme.{dark,darker,light,toggle}, transparencyGrid
- `window.` (11): closeAll, closePanel, dockAction, dockPanel, floatPanel, maximizeApp, maximizePanel, nextPanel, panelWindow, previousPanel, toggleCompTimeline
- `viewer.` (8): nudge.{down,down10,left,left10,right,right10,up,up10}
- `playback.` (5): preview.altNumpad0, preview.shiftNumpad0, preview.shiftSpacebar, ramPreview, stop
- `app.` (4): compSettings, home, newComp, solidSettings
- `flowchart.` (2): graph, options
- `markers.` (1): dialog

**Commands that matter for an asset browser** (paths under `crates/engine/src/`):

| Command | Params | Defined at |
| --- | --- | --- |
| `file.open` | `{path}` | `commands/file.rs:732` |
| `file.import` | `{paths, sequence?…}` | `commands/file.rs:738` |
| `file.importProject` | `{path}` → `{folder, items, comps, rootComps, primaryComp, missingMedia}` | `commands/import_project.rs:135` |
| `comp.info` | `{comp?}` | `commands/query.rs:214` |
| `project.summary` | `{}` | `commands/query.rs:213` |
| `app.capabilities` | `{}` → version, counts, export formats | `commands/query.rs:212` |
| `comp.setPosterTime` | `{}` | `commands/comp.rs:295` |
| `comp.saveFrameAsPsd` / `comp.saveFrameAsExr` | `{path, comp?, time?, scale?}` | `commands/frame_export.rs:222` / `:231` |
| `mediaBrowser.fileInfo` | `{path}` → kind, size, dates, footage metadata | `commands/panels_cmds.rs:255` (implementation at `:122`) |
| `item.metadata` | `{item?}` | `commands/panels_cmds.rs:256` |
| `mediaBrowser.list` / `mediaBrowser.import` | `{path?, importableOnly?}` / `{paths, sequence?, addToComp?…}` | `commands/panels_cmds.rs:233` / `:238` |
| `footage.missing` / `footage.relink` / `file.relinkFootage` | — | `commands/relink_moved.rs:230-238`, `commands/relink.rs:179` |
| `renderQueue.add` | `{comp?, template?, format?, output?…}` | `commands/render_queue.rs:998` |
| `renderQueue.render` | `{wait?}` | `commands/render_queue.rs:1048` |
| `renderQueue.list`, `.stop`, `.formats`, `.templates`, `.applyTemplate` | — | `commands/render_queue.rs:1049-1091` |
| `render.preRender` | `{comp?, output?, format?}` | `commands/render_queue.rs:1044` |
| `file.watchFolder` / `file.watchFolder.poll` | `{folder}` → renders queued `.ecproj` files and writes `.status.json` | `commands/watch_folder.rs:136-145` |
| `templates.list` | `{thumbnails?}` → 256×144 hex RGB | `templates.rs:910` |
| `templates.thumbnail` | `{id}` | `templates.rs:915` |
| `templates.create` / `templates.saveAs` | — | `templates.rs:917` / `:926` |
| `essential.exportTemplate` / `importTemplate` / `templateInfo` | `{path (.ectemplate)…}` | `commands/essential.rs:1028` / `:1037` / `:1061` |
| `anim.savePreset` / `anim.applyPreset` | `.ecpreset` | `commands/animation.rs:474` / `:482` |
| `file.exportLottie` / `file.importLottie` | — | `commands/lottie.rs:57` / `:65` |
| `file.exportTimeline` / `file.importTimeline` | `{comp?, path, format: xml\|fcpxml\|otio\|edl\|aaf\|omf, prerender?}` | `commands/timeline.rs:219` / `:210` |
| `file.runScript` | `{path (.jsx\|.jsonl) \| steps}` | `commands/file_more.rs:1014` |
| `engine.batch` | `{steps, atomic?}` | `commands/batch.rs:107` |
| `edit.editOriginal` | — | `commands/edit.rs:887` |
| `help.sibling` | `{app}`; does not include SoundCraft | `commands/help.rs:46` |

### 4. MCP server

- **Transport:** stdio JSON-RPC, protocol 2025-06-18 / 2025-03-26 / 2024-11-05 (`crates/automation/src/server.rs:14`).
- **Start:** `effectcraft-cli mcp` (headless) or `mcp --bridge PORT` (drives the GUI). The repo's `.mcp.json` has both.
- **Auth:** none. The `--automation-read-root` / `--automation-write-root` flags confine file access and refuse scripts and UI tools while set (`docs/mcp.md:6-17`).
- **Tools (33)** (`tools.rs:542-1009`):
  - list_commands, describe_command, execute_command, run_script, get_project, get_comp, get_layer, get_property, set_property, add_keyframe, add_effect, list_effects, list_fonts, get_state, render_frame, open_project, save_project, undo, redo, history, script_ui, batch, command_list, command_run, command_batch, render_preview, doc_inspect.
  - Bridge-only: screenshot, ui_inspect, ui_elements, ui_click, ui_drag, ui_key, ui_type, ui_set, control.
- **Mirrors the registry:** yes.
- **`render_frame`:** `{comp?, time?, max_side? (default 960), path?, inline?, transparent?}` (`tools.rs:728-745`). Headless it renders in-process; bridged it calls the GUI's `render.frame` (`backend.rs:105-131`).
- **Resources:** `effectcraft://document`, `effectcraft://commands` (`server.rs:224`). No prompts (`:242`).
- **Progress and cancellation** are supported for `renderQueue.render` (`docs/mcp.md:54-85`).

### 5. Control channel

- **Transport:** NDJSON over TCP, bound to 127.0.0.1 only (`apps/effectcraft/src/control_server.rs:25-34`). Lines up to 4 MiB, at most 16 connections (`:21-23`), 60 s timeout (`:63`). No authentication (`docs/control-protocol.md:26`).
- **Methods** (`crates/ui-egui/src/control.rs:171-460`):
  - `engine.execute`, `ui.menu.invoke`, `engine.commands`, `ui.menu.list`
  - **`render.frame {comp?, time?, max_side?, path?, base64?}`** (`:441`)
  - `ui.inspect`, `ui.elements`, `ui.set`, `ui.panel.show/close`, `ui.playback`
  - Input: `ui.click`, `ui.move`, `ui.drag`, `ui.scroll`, `ui.key`, `ui.type`
  - Locators: `ui.viewer.locate/hit`, `ui.timeline.locate`
  - `ui.screenshot`, `ui.resize`, `ui.focus`, `app.quit {force?}`
- **Example:** `{"id":1,"method":"engine.execute","params":{"command":"file.open","params":{"path":"/a.ecproj"}}}` returns `{"id":1,"ok":true,"result":…}`.
- **Can target the running GUI:** yes. I did not verify whether opening over the channel guards unsaved changes.

### 6. Library crates (not on crates.io; FilmCraft is pinned by git rev at `Cargo.toml:78-86`)

| Crate | Layer | Use | Key API |
| --- | --- | --- | --- |
| `effectcraft-project` | L2 | Read comps, sizes and `poster_time` without rendering | `Project` (`crates/project/src/lib.rs:1349`), `SCHEMA_VERSION=2` (`:34`), `saved_by()` (`:43`), `poster_time` (`:449`) |
| `effectcraft-render` | L3 | Quick poster for comps without footage | `render_frame(project, comp, t, scale)` (`crates/render/src/lib.rs:2340`); uses `NoFootage`, so footage layers come out blank |
| `effectcraft-engine` | L4 | In-process poster | `render_rgba8` (`lib.rs:1041`), `render_rgba8_alpha` (`:1099`) |
| **`effectcraft-host`** | L4 | **Best place to embed for accurate posters** | `host::session()` with media, expressions, scripting and export wired in (`crates/host/src/lib.rs:83-98`) |
| `effectcraft-automation` | L5, no UI deps | Same API headless or against the GUI | `Backend::headless` / `bridge` (`backend.rs:33` / `:38`), `exec` (`:63`), `render_with` (`:105`) |
| `effectcraft-media` | L3 | Metadata probe | `probe(path)`, `probe_bytes`, `probe_model` (`crates/media/src/probe.rs:17-70`) |
| `effectcraft-svg` / `effectcraft-pdf` | L2 | Alternative SVG/AI/EPS rasterizer | — |

### 7. Formats

- **`.ectemplate`:** a ZIP containing `manifest.json` (format `effectcraft-template` v1), `project.ecproj`, its media, and `poster.png` (≤320 px) (`essential.rs:847-868`).
- **Presets:** `.ecpreset` animation presets, composition presets, and Render Settings / Output Module templates.
- **FilmCraft:** see finding 5.

### 8. Gaps and opportunities (owner: EffectCraft)

| Need | Today | Suggested fix |
| --- | --- | --- |
| Open in a running instance | Control channel `file.open`, only if launched with `--control` | Single-instance forwarder; add `.ecproj` to WiX and Linux MIME |
| Poster frame | `render-frame`, MCP `render_frame`, `render_rgba8` | Add a `--poster` flag that uses `comp.poster_time`, and `--all-comps` |
| Template thumbnail | `poster.png` inside the ZIP; `templates.thumbnail` returns hex RGB | Return PNG base64 instead |
| Metadata probe | `mediaBrowser.fileInfo`, `info --json` | Add `project.peek {path}` that reads a project without opening it |
| Batch rendering | `render --queue`, `renderQueue.*`, `file.watchFolder` | Nothing needed |
| Apply a template | `render --template T --values V`, `templates.create` | Document the `--values` schema |

---

## VectorCraft (v0.11.0, 0a33cff3f6e2a1e9e672f4c3816f58cd8d3d032e)

### 0. What it is

VectorCraft is a clean-room reimplementation of Illustrator (`README.md:14`).

- **Native format:** `.vectorcraft`, versioned JSON v1–4, optionally gzipped, with an optional embedded preview PNG and PDF copy (`crates/format/src/lib.rs:1-31`).
- **Reads** (`crates/engine/src/cmd/fileio/mod.rs:592-618`): vectorcraft, drawcraft, svg, svgz, pdf, ai, ait, png, jpg, gif, webp, tif, bmp, psd, psb, vctemplate, dxf, emf, wmf, eps, af, afdesign, afpub.
- **Writes:** the same formats except Affinity, plus png8 and tga (`:465-635`).
- **License:** MIT OR Apache-2.0 (`Cargo.toml:9`). **Platforms:** macOS, Windows, Linux, FreeBSD, Web.
- **Layers** (`docs/architecture.md:22-29`):

| Layer | Crates |
| --- | --- |
| L0 | geom, color, affinity |
| L1 | doc |
| L2 | pathops, brush, trace, text, effects, plugins, three-d |
| L3 | render, svg, pdf, format, eps, cad, metafile |
| L4 | tools |
| L5 | engine |
| L6 | ui-egui, mcp |
| L7 | apps |

### 1. Desktop launch

- **Usage:** `vectorcraft [--control <port> [--automation-read-root D] [--automation-write-root D]] [--in-window-menus] [files…]` (`apps/vectorcraft/src/main.rs:3-15`, parser `:620-700`). `--control` takes 1–65535 and may appear only once (`automation_args.rs:61-72`).
- **Environment variables:** `VECTORCRAFT_CONTROL_PORT`, `VECTORCRAFT_AUTOMATION_READ_ROOT` / `_WRITE_ROOT` (`main.rs:625`), `VECTORCRAFT_IN_WINDOW_MENUS`, `VECTORCRAFT_NO_PREFS`, `VECTORCRAFT_CONFIG_DIR`, `WGPU_POWER_PREF` / `WGPU_ADAPTER_NAME`.
- **Single instance:** none. On macOS, Finder opens are routed to the running app through an injected `openURLs` handler (`open_documents.rs:1-13`).
- **Windows installer:** `vectorcraft-<ver>-windows-<arch>.msi`, per-machine (`packaging/windows/vectorcraft.wxs:20-24`), installed to `ProgramFiles6432Folder\VectorCraft` (`:40-41`).
  - Binaries: `vectorcraft.exe` (`:113`) and `vectorcraft-cli.exe` (`:124`). An App Paths key is registered (`:119`).
  - No file associations. The Linux MIME file is empty, and the `.desktop` file declares only svg and pdf (`.desktop:14`).

### 2. CLI (`apps/vectorcraft-cli/src/main.rs:67-113`)

| Subcommand | Key arguments / behavior | Lines |
| --- | --- | --- |
| `mcp` | `[--connect ADDR \| --headless] [roots]`; by default connects to `127.0.0.1:7979` and falls back to headless | `:196-203` |
| `run` | `[--in F] [--cmd ID [--params JSON]]… [--export F]… [--scale N]` | `:285` |
| `commands` | Prints the catalog as JSON | `:214` |
| `convert IN OUT` | `[--scale] [--artboard\|--range] [--outline-text] [--decimals] [--replace-lossy] [--editing-data]` | `:221-261` |
| `info FILE` | Import warnings, `file.info`, artboards, object kinds, fonts | `:263-278` |
| `bench` / `perf` | Timings | — |

There is no script-file runner. Exit codes: 0 or 1 (`:147-153`).

### 3. Command registry

- `CommandSpec` is defined at `crates/engine/src/cmd/mod.rs:103-116`.
- Macros: `cmd!` and `cmd!(query …)` (`:186-193`). `style_cmds!` generates the 16 `charStyle.*` / `paraStyle.*` commands (`cmd/textstyles.rs:53-112`). `command_specs()` is at `:196`.
- 285 UI commands live in `crates/ui-egui/src/menus.rs:54-…`. They are also exposed through `engine.commands` (`control.rs:131`).
- `file.open` and `file.save` are UI-layer commands (`menus.rs:61-71`, `:979-988`). Headless callers use `document.open` / `document.save` / `document.export` instead.

**Engine commands (743, grouped by namespace):**
- `object.` (133): align, arrange.{bringForward,bringToFront,sendBackward,sendToBack,sendToCurrentLayer}, blend.{expand,info,make,options,release,replaceSpine,reverseFrontToBack,reverseSpine,spine.addAnchor,spine.moveAnchor,spine.removeAnchor,spine.reshapeSegment}, clippingMask.{editContents,editMask,make,release}, compoundPath.{make,release}, convertDocumentColorMode, createObjectMosaic, createTrimMarks, cropImage, distort, distribute, distributeSpacing, envelope.{editContents,expand,info,makeWithMesh,makeWithTopObject,makeWithWarp,options,release,resetWithMesh,resetWithWarp,setMeshPoint}, exitIsolation, expand, expand.info, expandBrush, expandShape, flattenTransparency, group, hide, hide.above, hide.otherLayers, isolate, liquify, lock, lock.above, lock.otherLayers, makePixelPerfect, maskImage, mesh.{addLine,create,deletePoint,expand,movePoint,setPointColor}, move, nudge, path.{addAnchorPoints,cleanUp,divideObjectsBelow,offsetPath,outlineStroke,simplify,splitIntoGrid}, pathfinder.{crop,divide,exclude,intersect,merge,minusBack,minusFront,outline,trim,unite}, pattern.{cancel,done,edit,make,saveCopy}, puppetWarp, puppetWarp.pins, rasterize, reflect, repeat.{expand,grid,mirror,options,radial,release}, resetBoundingBox, rotate, scale, setBounds, setLiveShape, setOverprint, setProps, shape.convertToShape, shear, showAll, slice.{clipToArtboard,combine,create,delete,deleteAll,divide,duplicate,fromGuides,fromSelection,make,move,options,release,select,setRect}, textWrap.{make,options,release}, transform, transformAgain, transformEach, ungroup, unlockAll, vectorHalftone
- `select.` (48): add, all, allOnArtboard, anchors, anchorsMany, editSaved, font, inverse, key, magicWand, nextAbove, nextBelow, none, object.{allOnSameLayers,areaText,bristleBrushStrokes,brushStrokes,clippingMasks,directionHandles,openPaths,pointText,slices,strayPoints,textObjects}, recall, reselect, same.{appearance,appearanceAttribute,blendingMode,fillAndStroke,fillColor,fontFamily,fontFamilyStyle,fontFamilyStyleSize,fontSize,graphicStyle,opacity,shapeType,strokeColor,strokeWeight,symbolInstance,textFillColor,textStrokeColor}, save, savedList, segments, set, toggle
- `path.` (32): appendAnchor, average, blob, close, convertAnchor, convertAnchors, create, curvature, curvatureEdit, cutAtAnchors, deleteAnchors, eraseRegion, eraseSegments, freehand, insertAnchor, join, joinScrub, knife, lineCut, mirrorCut, moveAnchors, rectCut, removeAnchor, removeAnchors, reshape, reshapeSegment, reverse, setAnchors, setFillRule, setHandle, smoothRegion, split
- `document.` (29): activate, dxfInfo, export, exportDxf, exportEps, exportForOffice, exportForScreens, exportForWeb, exportForWeb.preview, exportPdf, exportSelection, exportSettings, find, formats, info, inspect, json, node, open, pdfInfo, pdfSettings, rasterEffectsSettings, resetRulerOrigin, rulerOrigin, save, serialize, setRulerOrigin, setUnits, setup
- `edit.` (28): assignProfile, clear, colorSettings, colors.{adjustBalance,blendFrontToBack,blendHorizontally,blendVertically,invert,overprintBlack,saturate,toCMYK,toGrayscale,toRGB}, copy, cut, duplicate, findNext, findReplace, paste, pasteInBack, pasteInFront, pasteInPlace, pasteOnAllArtboards, pasteWithoutFormatting, redo, undo, undoGroup.begin, undoGroup.end
- `text.` (27): addFontFiles, areaOptions, create, createInPath, discardEmpty, editRange, findFontFiles, fitHeadline, fontList, fonts, getRange, insertInline, missingFonts, replaceFont, rescanFonts, reshapeArea, setFormat, setRangeStyle, setStyle, setText, tabs.{clear,get,set}, thread.{create,info,releaseSelection,remove}
- `layer.` (26): clippingMask.toggle, collectInNew, delete, duplicate, flatten, hideOthers, highlight, locate, lockOthers, merge, move, new, newSublayer, outlineOthers, pasteRemembersLayers, previewAll, releaseToLayers, releaseToLayersBuild, reverse, selectAll, setCurrent, setProps, showAll, target, template, unlockAll
- `perspective.` (25): attach, draw, editText, grid.{define,get,lock,lockStation,preset,rulers,set,show,snap}, move, nudge, plane.{matchObject,move,set}, presets.{delete,export,import,list,save}, release, transform, widget.options
- `file.` (24): close, closeAll, documentColorMode, formatOptions, info, list, new, newFromTemplate, newPresets, newPresets.delete, newPresets.save, package, place, place.info, place.queue, print, recovery.{discard,list,restore,save}, revert, saveAs, saveAsTemplate, saveCopy
- `swatch.` (23): addUsedColors, delete, duplicate, edit, editGroup, library.{add,copyToUser,get,list,load,save}, list, merge, move, new, newGroup, resetDefaults, setSpot, sortByKind, sortByName, spotOptions, ungroup, unused
- `paint.` (22): complement, default, editGradient, freeform.{addLine,addPoint,deletePoint,get,selectPoint,setPoint,splitLine}, invert, lastColor, lastGradient, none, proxies, recent, sampleColor, setFill, setGradientGeom, setStroke, swap, toggleActive
- `graphicStyle.` (18): addFromLibrary, apply, breakLink, delete, duplicate, libraries, library, list, loadLibrary, merge, move, new, redefine, rename, saveLibrary, setOptions, sortByName, unused
- `transparency.` (17): disableOpacityMask, editOpacityMask, enableOpacityMask, info, linkOpacityMask, makeOpacityMask, opacityMaskInfo, releaseOpacityMask, set, setOpacityMask, stopEditingOpacityMask, toggleNewMasksClipping, toggleNewMasksInverted, togglePageIsolatedBlending, togglePageKnockoutGroup, unlinkOpacityMask, viewOpacityMask
- `appearance.` (16): addEffect, addFill, addStroke, clear, copyFrom, duplicateItem, moveItem, newArt, reduceToBasic, removeItem, setActiveItem, setItem, setNewArtBasic, showAllHidden, targetContents, transfer
- `view.` (16): drawMode, guides.{clear,lock,make,release}, overprintPreview, printTiling, proofColors, proofSetup, saved.{edit,list,new}, separationsPreview, slices.hide, slices.lock, transparencyGrid
- `library.` (15): add, create, createGroup, delete, deleteGroup, export, get, import, list, moveItem, removeItem, rename, renameGroup, setCurrent, use
- `type.` (14): changeCase, convertToAreaType, convertToPointType, createOutlines, fillPlaceholder, fitHeadline, insert, orientation.horizontal, orientation.vertical, pathOptions, size.decrease, size.increase, smartPunctuation, step
- `symbol.` (13): adjust, breakLink, delete, duplicate, edit, list, new, place, replace, selectInstances, setCurrent, spray, update
- `artboard.` (12): convertToArtboards, copy, cut, delete, duplicate, fitToArt, fitToSelection, move, new, rearrange, reorder, setProps
- `clipboard.` (12): conflicts, exportPdf, exportPng, exportSvg, exportText, flavours, importEmf, importEps, importImage, importPdf, importSvg, importText
- `links.` (11): check, embed, findFiles, goTo, info, list, placementOptions, relink, unembed, update, updateChanged
- `brush.` (10): apply, delete, duplicate, freehand, get, list, new, options, remove, setCurrent
- `dataset.` (10): capture, delete, list, new, next, prev, rename, select, set, update
- `shape.` (10): arc, ellipse, flare, line, polarGrid, polygon, rectangle, rectangularGrid, spiral, star
- `stroke.` (10): set, setAdvanced, widthPoint.{copy,remove,set}, widthProfile.{add,delete,list,reset,set}
- `print.` (9): presets.{delete,export,import,list,save}, preview, setup, tiling, tiling.set
- `variable.` (9): bind, define, delete, highlight, list, makeTextDynamic, makeVisibilityDynamic, rename, unbind
- `charStyle.` / `paraStyle.` (8 each): apply, delete, duplicate, list, new, redefine, rename, setAttrs
- `effect.` (7): apply, duplicate, expandAppearance, list, move, remove, setParams
- `livePaint.` (7): expand, fill, info, make, merge, release, strokeEdge
- `assets.` (6): add, export, list, remove, rename, settings.set
- `flattener.` (6): presets.{delete,export,import,list,save}, preview
- `guide.` (6): add, list, move, remove, select, setLayer
- `imageTrace.` (6): expand, make, makeAndExpand, presets, release, setView
- `plugin.` (6): info, install, list, reload, remove, run
- `color.` (5): convert, gamutCheck, harmony, loadProfile, plates
- `graph.` (5): create, design, marker, setData, setType
- `help.` (5): appPage, discord, github, links, website
- `pdf.` (5): preset.{delete,export,import,list,save}
- `colorTheme.` (4): addToSwatches, delete, list, save
- `pattern.` (4): delete, list, options, transform
- `prefs.` (4): get, list, reset, set
- `recolor.` (4): apply, colors, randomize, reduce
- `shaper.` (4): expand, release, scribble, select
- `webExport.` (4): presets.{delete,list,save}, settings
- `css.` (3): export, generate, selection
- `importMatches.` (3): list, reset, set
- `attributes.` (2): info, set
- `magicWand.` (2): options, set
- `shapeBuilder.` (2): merge, regions
- Single-command namespaces: `command.batch`, `eyedropper.setOptions`, `gradient.selectStop`, `image.info`, `node.move`, `script.check`, `shortcutSet.read`, `slice.list`

**UI-layer commands (285):**
- `ui.` (67): blendOptions, brushOptions, checkScript, colorBalanceDialog, colorGuideLimit, colorGuideOptions, colorPicker, corners, cropImage, dxfOptionsDialog, epsOptionsDialog, expandDialog, fileInfoDialog, findFontsInFolder, findLinksInFolder, flattenTransparencyDialog, flattenerPresetsDialog, flattenerPreview, graphicStyleOptions, installPlugin, layerOptions, layersExpand, layersPanelOptions, menuDialog, mergeGraphicStyles, missingFontsDialog, newColorGroup, newLayer, newSwatch, packageDialog, paramDialog, pdfPresetDialog, pdfPresetsDialog, perspectiveGridDialog, perspectivePlane, perspectivePresetsDialog, perspectiveUserPreset{1-3}.{1-5}, placementOptionsDialog, printPresetDialog, printPresetsDialog, rasterEffectsSettingsDialog, recolorDialog, saturateDialog, saveGraphicStyleLibrary, savePdfDialog, savePerspectivePreset, saveSwatchLibrary, shortcutImportDialog, spotColors, swatchOptions, symbolOptions, tileEdgeColor, widthPointEdit
- `window.` (51): brightness, collapseDock, control, dock, editToolbar, floatTools, graphicStyleLibrary, graphicStyleLibrary.other, newWindow, panel, panel.{activate,close,dock,float,layout,move}, swatchLibrary, swatchLibrary.other, taskBar, taskBar.pin, taskBar.reset, toolbar, toolbarAdvanced, toolbarColumns, userGraphicStyleLibrary1-10, userSwatchLibrary1-10, workspace, workspace.{delete,list,manage,new,rename,reset}
- `file.` (48): clearRecent, documentSetup, export.pdf, export.png, export.svg, exportAs, exportForScreens, exportSelection, newDialog, open, openRecent1-30, place, print, recentFiles, reveal, save, saveForWeb, saveForWeb.browser, showPackage
- `view.` (37): actualSize, artboards, boundingBox, cornerWidget, edges, fitAll, fitArtboard, goToArtboard, goto1-10, gradientAnnotator, grid, guides, outline, pixelPreview, preciseCursors, presentation, rotateReset, rulers, screenMode, setZoom, smartGuides, snapToGrid, snapToPixel, snapToPoint, textThreads, trimView, zoomIn, zoomOut
- `select.` (25): recall1-25
- `type.` (24): alignCenter, alignLeft, alignRight, bold, findFont, hiddenCharacters, italic, justifyAll, justifyLeft, recentFont1-15
- `shortcuts.` (7): conflicts, export, import, list, preset, reset, set
- `toolbar.` (4): addTool, removeTool, reset, show
- `app.` (3): home, language, quit
- `effect.` (3): applyLast, dialog, last
- `tool.` (3): options, select, setOption
- `css.` (2): copy, exportFile
- `edit.` (2): keyboardShortcuts, preferences
- `help.` (2): about, commandPalette
- `links.` (2): editOriginal, reveal
- `print.` (2): printerSetup, printers
- Single-command namespaces: `attributes.openUrl`, `docInfo.save`, `plugin.dialog`

**Commands that matter for an asset browser** (paths under `crates/engine/src/cmd/`):

| Command | Params | Defined at |
| --- | --- | --- |
| **`file.place.info`** | `{path\|name+dataBase64, page?, crop?, thumbnail?: px}` → `{name, format, width, height, pixelWidth?, ppi?, colorMode?, warnings, thumbnailBase64 (≤512)}` | `place/mod.rs:65` |
| **`document.pdfInfo`** | `{path, password?, thumbnail?: page, thumbnailSize?: 160, cropTo?}` → `{pages, pageInfo[], thumbnail}` | `fileio/mod.rs:158` |
| `document.open` | `{path, pages?, cropTo?, textAs?, layers?, editingData?, colorMode?, dxf?}` | `fileio/mod.rs:86` |
| `document.export` | `{path?, format?, artboard?, range?, ppi?, scale?, background?…}` | `fileio/mod.rs:113` |
| `document.serialize` | `{format?…}` → `{dataBase64}` (no file written) | `fileio/mod.rs:104` |
| `document.formats` | — | `fileio/mod.rs:149` |
| `file.list` | `{path}` | `fileio/mod.rs:95` |
| `document.save` / `file.saveAs` / `file.saveCopy` / `file.saveAsTemplate` / `file.newFromTemplate` | `preview?` (embed thumbnail) | `fileio/save.rs:710-746` |
| `document.info` | `{category?, format?: text}` | `docinfo.rs:17` |
| `file.info` | XMP-style title, author, keywords, rating | `docmenu.rs:225` |
| `image.info` | — | `place/mod.rs:74` |
| `document.exportForScreens` / `document.exportPdf` | — | `fileio/mod.rs:131`, `fileio/pdf.rs:108` |
| `assets.export` | — | `assets.rs:47` |
| `command.batch` | `{commands}`, one undo step | `fileio/mod.rs:140` |
| `library.get` / `library.export` / `library.import` | graphics with PNG thumbnails | `library.rs:451-568` |
| `links.list` / `links.check` / `links.relink` | — | `links.rs:38-110` |
| `file.package` | — | `package.rs:23` |
| `file.new` / `file.newPresets` | — | `edit.rs:17`, `newdoc.rs:15` |
| UI: `file.open {path}`, `file.export.png`, `file.reveal`, `app.home` | — | `ui-egui/src/menus.rs:61-…` |

### 4. MCP server

- **Transport:** stdio JSON-RPC, protocol 2026-07-28 / 2025-06-18 / 2025-03-26 / 2024-11-05 (`crates/mcp/src/server.rs:13-15`). Auth: none; optional automation roots.
- **Tools (32)** (`tools.rs:79-461`): command_list, command_run, command_batch, doc_inspect, render_preview (`max_side` 1..4096, default 1024, `:113-122`), ui_inspect, ui_screenshot, list_commands, run_command, inspect_document, inspect_ui, select_tool, pointer_gesture, draw_path, draw_shape, set_paint, press_key, type_text, invoke_menu, open_panel, screenshot, open_file, save_file, export, add_text, apply_effect, pathfinder, transform, create_graph, text_wrap, undo, redo.
- **Mirrors the registry:** yes.
- **Resources:** `vectorcraft://document`, `vectorcraft://document/json`, `vectorcraft://commands` (`resources.rs:15-20`).
- **Resource templates:** `object/{id}`, `command/{id}`, `effect/{id}`, `swatch/{name}` (`:45-72`).
- **Prompts:** poster, icon-set, recolor, trace-and-style, export-set (`prompts.rs:127-316`).

### 5. Control channel

- **Transport:** NDJSON over 127.0.0.1 (`apps/vectorcraft/src/control_server.rs:60-69`). Lines up to 4 MiB, at most 16 connections (`:26-28`). Timeouts: 5 min idle, 30 s per line, 60 s per reply. No authentication; optional automation roots (`docs/control-protocol.md:22-80`).
- **Methods** (`crates/ui-egui/src/control.rs:198-451`):
  - `engine.execute`, `engine.commands`, `document.inspect`
  - `ui.*`: pointer, key, click, drag, wheel, text, set, dialog, screenshot
  - **`ui.render {path?, scale?, artboard?, data?}`** renders the active document (`:415-440`)
  - **`app.open {path}`** (`:442`), `app.save`, `app.export`, `app.quit`
- **Example:** `{"id":2,"method":"engine.execute","params":{"command":"file.place.info","params":{"path":"/a.svg","thumbnail":256}}}` returns `{…,"thumbnailBase64":"…"}`.
- **Can target the running GUI:** yes. The MCP server connects to it by default.

### 6. Library crates (path deps only; craft-ui is only needed by ui-egui)

| Crate | Layer | Use | Key API |
| --- | --- | --- | --- |
| `vectorcraft-format` | L3 | Instant preview, when present | `preview(bytes)` (`crates/format/src/lib.rs:581`, ≤256 px), `load_info`, `pdf_content`, `sniff` |
| `vectorcraft-render` | L3 | Raster thumbnail of a loaded document | `Renderer::render` (`crates/render/src/lib.rs:603`), `render_region` (`:782`), `render_thumbnail` (`:805`) |
| `vectorcraft-svg` | L3 | SVG to document | `import` (`crates/svg/src/lib.rs:497`) |
| `vectorcraft-pdf` | L3 | PDF / `.ai` import and page info | `import`, `info`, `has_illustrator_data` (`crates/pdf/src/lib.rs:70-75`) |
| `vectorcraft-eps` | L3 | EPS previews | `thumbnail`, `native` (`crates/eps/src/lib.rs:381-408`) |
| `vectorcraft-affinity` | L0 | Affinity files | — |
| **`vectorcraft-engine`** | L5, no UI deps | **Best place to embed**: all formats through `file.place.info` | — |
| `vectorcraft-mcp` | L6, no UI deps | Same API in-process or against the GUI | `Headless::new` / `with_document` (`headless.rs:101-112`), `Remote`, `Backend` |

### 7. Formats

- **Templates:** `.vctemplate`, plus `.ait` on open.
- **Presets and libraries:** New Document presets, `.vcpdfpresets`, `.vcprintpresets`, Save for Web presets, `.vcstyles` graphic style libraries, and `.vclibrary` libraries (which carry per-graphic thumbnails).
- **FilmCraft interchange:** none.

### 8. Gaps and opportunities (owner: VectorCraft)

| Need | Today | Suggested fix |
| --- | --- | --- |
| Open in a running instance | `app.open` over the control channel; macOS `openURLs` | Single-instance forwarder; register file associations in WiX and Linux MIME |
| Thumbnail | `file.place.info`, `document.pdfInfo`, `convert --scale`, `format::preview` | Add `vectorcraft-cli thumb IN OUT --max-side N`; consider `preview: true` as the default on native save |
| Metadata probe | `info`, `document.info`, `file.info` | Nothing needed |
| Batch export | `run --export`, `command.batch`, `assets.export` | Optional `convert --glob` |
| Apply a template | `file.newFromTemplate` | Nothing needed |
| Port 7979 | Shared with SoundCraft's default | Publish the port in a port file in the config dir |

---

## SoundCraft (v0.7.0, 3a28a657692c1b98b28c276d8f61df9471074f0f)

### 0. What it is

SoundCraft is a clean-room reimplementation of Pro Tools: a full DAW (`README.md:13-20`). On Crafting App status, see finding 5.

- **Formats:**
  - Session: `.scraft` (JSON, `format_version` 1) with an `Audio Files/` folder beside it, or a `.scraftz` bundle (`crates/model/src/session.rs:300-346`, `crates/engine/src/io.rs:866-873`, `:1152`).
  - Audio in: WAV/BWF/RF64, AIFF, FLAC, MP3, OGG, AAC, ALAC, CAF and audio from MP4/MOV. Audio out: WAV, AIFF, FLAC (`README.md:132-135`).
- **License:** MIT OR Apache-2.0. **Platforms:** the same set as the other two. **Status:** pre-alpha.
- **Layers** (`xtask/src/main.rs:20-37`): 0 time, audio-io, midi, video (all standalone); 1 dsp (standalone), clap-host, vst3-host, au-host; 2 model; 3 mix; 4 engine, playback; 5 automation; 6 ui-egui.

### 1. Desktop launch

- **Usage:** `soundcraft [--demo|--sample] [--control <port>] [--no-audio] [--version] [session.scraft | audio…]` (`apps/soundcraft/src/main.rs:3`, `:210-228`). Unknown flags are silently ignored (`:226`).
- **Control port:** `--control 0` picks a free port and prints `SOUNDCRAFT_CONTROL_PORT=n` (`control_server.rs:35-37`).
- **File routing** (`main.rs:108-121`): `.scraft` goes to `session.open` (it asks first if there are unsaved changes), `.mid` to `file.import_midi`, everything else to `file.import_audio`.
- **Environment variables:** `SOUNDCRAFT_CONTROL_PORT`, `SOUNDCRAFT_NO_PREFS`, `SOUNDCRAFT_CONFIG_DIR`, `RUST_LOG`, `WGPU_BACKEND`.
- **Single instance:** none, and no macOS Apple-event handling either.
- **Windows installer:** `soundcraft-<ver>-windows-<arch>.msi`, per-machine (`packaging/windows/soundcraft.wxs:13-17`), installed to `ProgramFiles6432Folder\SoundCraft` (`:33-34`). Binaries: `soundcraft.exe` (`:46`) and `soundcraft-cli.exe` (`:86-87`).
  - File associations: ProgId `SoundCraft.Session` for `.scraft` (`:53-57`), and "Open with" for wav, aif, aiff, flac, mp3, mid and midi (`:60-81`).
  - The Linux MIME type `application/x-soundcraft-session` is declared.

### 2. CLI (`apps/soundcraft-cli/src/main.rs:3-19`)

| Subcommand | Key arguments / behavior | Lines |
| --- | --- | --- |
| `info FILE` | Audio files give JSON `{format, sample_format, sample_rate, channels, frames, seconds, bwf}`; a `.scraft` gives text | `:98-121` |
| `convert IN OUT` | `[--bit-depth] [--rate] [--normalize]` | `:123` |
| `run` | `[--in\|--demo] [--cmd ID[=JSON]]… [--bounce OUT] [--start --end] [--save] [--inspect]` | `:245` |
| `script FILE` | One `ID JSON` per line; `[--keep-going]` | `:270-302` |
| `commands [FILTER]` / `describe ID` | — | `:304-326` |
| `app [--port P] METHOD [JSON]` | Default port 7979 | `:361-387` |
| `mcp` | `[--connect PORT] [--demo] [roots]` | `:389-420` |
| `parity` / `plugins` | — | — |

- **Exit codes:** 0 or 1 (`:44-47`).
- **Small bug:** `app`'s `is_control_method` list is missing `ui.scroll`, `ui.zoom` and `ui.dock`, so those get sent as engine commands (`:329-350`). Fix: add them to the list.

### 3. Command registry

- `CommandSpec` is defined at `crates/engine/src/cmd/mod.rs:49-64`.
- Macros: `cmd!` with `query` / `noundo` / `view` variants (`:93-103`), plus `toggle!` (`options.rs:9`), `flag!` / `radio!` (`view_more.rs:11-22`) and `option!` (`setup_more.rs:12`). `mix.insert_state` is hand-written (`plugin_state.rs:10`).
- `command_specs()` is at `:105-141`. 66 UI commands are in `crates/ui-egui/src/menus.rs:9-75`.

**Engine commands (478, grouped by namespace):**
- `view.` (96): automation_composite_playlist, automation_trim_playlist, clip_all_channels, clip_ara_note_overlay, clip_channel_name, clip_copy_move_indicators, clip_effects_status, clip_gain_info, clip_gain_line, clip_name, clip_overlap_shadows, clip_overwrite_indicator, clip_processing_state, clip_rating, clip_scene_take, clip_sync_point, clip_time_{current,none,original,user}, clip_transparency, dropped_video_frame_indicator, edit_{all,comments,inserts_ae,inserts_fj,instruments,io,marker_controls,mic_preamps,minimal,object,pinned_tracks,realtime_properties,sends_ae,sends_fj,track_color}, expanded_send_{a..j}, expanded_sends_all, expanded_sends_none, flags, grid, grid_lines, main_counter, marker_folder_lines, marker_ruler_lane_color, marker_ruler_lines, marker_track_lane, marker_track_lane_color, midi_editor_clip_effects, midi_input_display, nudge, ruler, ruler_all, ruler_all_markers, ruler_key_staff, ruler_markers2-5, ruler_minimal, ruler_tempo_editor, ruler_timecode2, scroll, sub_counter, track_number, track_transcription_lane, transport_{ableton_link,counters,expanded,midi_controls,output_meters,synchronization}, waveform_{outlines,overlapped_crossfades,peak,power,rectified,zoom}, zoom_by, zoom_fit, zoom_in, zoom_out, zoom_preset, zoom_set, zoom_to_selection, zoom_toggle
- `edit.` (93): automation_coalesce_clip_gain_to_volume, automation_coalesce_volume_to_clip_gain, automation_glide_pan, automation_glide_to_all, automation_trim_to_all, clear, clear_all_automation, clear_clip_effects, clear_clip_gain, clear_pan_automation, clear_plugin_automation, consolidate, copy, copy_all_automation, copy_audio_as_midi, copy_clip_effects, copy_clip_gain, copy_pan_automation, copy_plugin_automation, copy_to_duplicate_playlist, copy_to_new_playlist, copy_to_target_playlist, cut, cut_all_automation, cut_clip_effects, cut_clip_gain, cut_pan_automation, cut_plugin_automation, double_selection, duplicate, duplicate_extend, extend_down_to_members, extend_selection_down, extend_selection_up, extend_up_to_members, fade_to_end, fade_to_start, fades_create, fades_delete, halve_selection, heal, insert_silence, marker_lane_extend_down, marker_lane_extend_up, marker_lane_move_down, marker_lane_move_up, mode, move_clips, move_selection_left, move_selection_right, move_to_duplicate_playlist, move_to_new_playlist, move_to_target_playlist, mute_clips, nudge, paste, paste_clip_effects, paste_clip_gain, paste_merge_markers, paste_merge_midi, paste_repeat_to_fill, paste_to_current_automation, play_timeline, redo, remove_selection_bottom, remove_selection_top, repeat, restore_last_selection, select, select_all, select_none, selection_to_timeline, separate, separate_at_transients, separate_on_grid, shift, shuffle_lock, snap_next, snap_previous, snap_to_grid, space_clips, strip_silence, tab, tce_to_timeline, timeline_to_selection, tool, trim_clip, trim_end_to_insertion, trim_start_to_insertion, trim_to_file_boundaries, trim_to_fill_selection, trim_to_selection, undo
- `track.` (65): bounce, bypass_delay, bypass_dynamics, bypass_eq, bypass_inserts, bypass_inserts_ae, bypass_inserts_fj, bypass_modulation, bypass_reverb, change_width, clear_clip_indicators, clear_trim_automation, coalesce_trim, coalesce_vca, color, comments, commit, convert_aux_to_folder, create_click, delete, designate_target_playlist, duplicate, elastic, extract_midi, folder_toggle, freeze, group, group_toggle, height, height_down, height_up, hide, input, inspect, load_preset, make_inactive, move, move_to_folder, move_to_new_folder, mute_sends, mute_sends_ae, mute_sends_fj, new, object, output, pin, playlist_delete_unused, playlist_duplicate, playlist_new, playlist_promote, playlist_select, remove_from_folder, rename, save_preset, scroll_to, set_record_to_input_only, show_target_playlist, split_into_mono, timebase, toggle_recent_playlist, trim_automation_write, ungroup, vca, view, write_midi_rtp
- `event.` (35): add_key_change, all_notes_off, beat_detective, change_duration, change_velocity, cut_time, extract_chords, flatten_performance, identify_beat, input_quantize, insert_time, meter, midi_note, midi_ops_window, midi_rtp, midi_track_offsets, move_song_start, quantize, remove_duplicate_notes, renumber_bars, restore_performance, retrospective_record, select_split_notes, step_input, tempo, tempo_constant, tempo_linear, tempo_ops_window, tempo_parabolic, tempo_remove, tempo_s_curve, tempo_scale, tempo_stretch, time_ops_window, transpose
- `options.` (35): auto_spot_clips, automation_follows_edit, calibration_mode, click, countoff, delay_compensation, destructive_punch, destructive_record, dynamic_transport, insertion_follows_playback, keyboard_focus, keyboard_lock, layered_editing, link_timeline_edit, link_track_edit, loop_playback, loop_record, low_latency_monitoring, marker_target_follows_selection, markers_follow_edit, midi_live_mode, midi_merge, midi_thru, mirrored_midi, post_roll, pre_fader_metering, pre_post_roll, pre_roll, prepare_dpe_tracks, quickpunch, scrolling, solo_mode, tab_to_transient, trackpunch, transport_online
- `clip.` (31): bring_to_front, capture, clear, color, conform_to_tempo, edit_lock, effects_bypass, effects_render, effects_set, elastic_properties, gain, gain_bypass, gain_nudge_down, gain_nudge_up, gain_render, group, identify_sync_point, inspect, loop, place_source, quantize_to_grid, rating, regroup, remove_pitch_shift, remove_warp, rename, send_to_back, time_lock, ungroup, ungroup_all, unloop
- `mix.` (29): automation_mode, clear_mutes, clear_solos, input_monitor, insert, insert_bypass, insert_move, insert_param, insert_params, insert_remove, insert_state, instrument, instrument_param, mute, new_bus, pan, phase_invert, record_arm, sampler_clear, sampler_load, send, send_level, send_remove, send_surround_pan, solo, solo_safe, surround_pan, trim, volume
- `setup.` (20): click_countoff, current_feet_frames, current_timecode, disk_allocation, external_timecode_offset, hardware, io, keyboard_shortcuts, main_format, midi_beat_clock, midi_input_devices, midi_input_filter, midi_studio, peripherals, playback_engine, preferences, reset_edit_state, session, transcription_settings, video_sync_offset
- `file.` (17): bounce_mix, bounce_stems, export_clip_groups, export_clips, export_midi, export_selected_tracks_as_session, export_session_text, export_sibelius, get_info, import_audio, import_clip_groups, import_midi, import_session_data, import_video, print_score, score_setup, send_to_sibelius
- `transport.` (13): fast_forward, go_to_end, half_speed, locate, pause, play, play_selection, record, rewind, rtz, status, stop, toggle
- `session.` (11): audio_health, close, format_time, inspect, new, open, revert, save, save_as, save_copy, save_template
- `automation.` (10): clear, clip_gain_to_volume, copy_to_send, set_point, thin, thin_all, volume_to_clip_gain, write_range, write_to_all, write_to_current
- `engine.` (6): clap_plugins, commands, history, parity, plugins, vst3_plugins
- `markers.` (6): add, delete, edit, next, previous, recall
- `midi.` (6): new_clip, note_add, note_delete, note_edit, notes, notes_move
- `video.` (3): list, offset, online
- Single-command namespaces: `app.quit`, `audiosuite.process`

**UI-layer commands (66):**
- `window.` (58): about, arrange_cascade, arrange_tile, arrange_tile_h, arrange_tile_v, audio_health, automation, beat_detective, big_counter, clip_effects, clip_effects_dock, clip_list, clip_list_view, close, color_palette, config_auto_update, config_new, config_update, configurations, disk_usage, edit, event_list, hide_floating, io_setup, memory_locations, metadata, midi_back, midi_editor, midi_front, midi_keyboard, midi_ops, mix, mix_separate, narrow_mix, playback_engine, renderer, rtp, score, search, session_info, shortcuts, system_usage, task_manager, tempo_ops, time_ops, toggle_mix_edit, track_list, transport, ui_customization, undo_history, universe, video, video_universe, workspace, workspace_close, workspace_front, workspace_presets, workspace_sounds
- `ui.` (7): audio_device, bounce_dialog, dock, dock.reset, new_tracks_dialog, set, theme
- `view.` (1): mix_section

**Commands that matter for an asset browser** (paths under `crates/engine/src/cmd/`):

| Command | Params | Defined at |
| --- | --- | --- |
| `session.new` | `{name?, sample_rate?, bit_depth?, template?: blank\|demo}` | `file.rs:11` |
| `session.open` | `{path}` → `{name, tracks, missing}` | `file.rs:12` |
| `session.save` / `.save_as` / `.save_copy` / `.save_template` / `.revert` | — | `file.rs:18-52` |
| `file.import_audio` | `{path\|paths, track?, at?, new_tracks?}` | `file.rs:58` |
| `file.import_midi` / `file.import_video` | — | `file.rs:66` / `:74` |
| `file.bounce_mix` | `{path, format?, bit_depth?, start?, end?, source?, normalize?, dither?}` | `file.rs:87` |
| `file.bounce_stems` | `{dir, format?…}` | `file.rs:88` |
| `file.export_clips` / `file.export_midi` / `file.export_session_text` / `file.export_selected_tracks_as_session` / `file.export_sibelius` / `file.print_score` | — | `file.rs:106-149` |
| `file.export_clip_groups` / `file.import_clip_groups` | `.scgrp` | `file.rs:179-185` |
| `file.get_info` | — | `file.rs:194` |
| `file.import_session_data` | — | `setup_more.rs:119` |
| `track.save_preset` / `track.load_preset` | — | `track_more.rs:61-63` |
| `session.inspect {detail?}`, `track.inspect`, `clip.inspect` | — | `query.rs:9-14` |
| `session.audio_health` | — | `health.rs:11` |
| `video.list` | — | `video.rs:31` |
| `audiosuite.process` | — | `audiosuite.rs:11` |
| `engine.plugins` / `engine.clap_plugins` / `engine.vst3_plugins` | — | `query.rs:28`, `clap.rs:11`, `vst3.rs:11` |
| `mix.sampler_load` | — | `sampler.rs` |

### 4. MCP server

- **Transport:** stdio JSON-RPC, protocol 2025-06-18 (`crates/automation/src/mcp.rs:7`).
- **Start:** `soundcraft-cli mcp [--demo]` (headless) or `--connect PORT` (drives the GUI).
- **Auth:** none. The automation roots can't be combined with `--connect`, and the desktop app has no root flags (`docs/mcp.md:20-69`).
- **Tools (16)** (`mcp.rs:20-97`): list_commands, execute, batch, inspect_session, new_session, open_session, save_session, import_audio, bounce_mix, parity. App-only: screenshot, ui_inspect, ui_set, menu_invoke, click, drag, key.
- **Mirrors the registry:** yes, through `execute` and `batch`.
- **Resources:** `soundcraft://session`, `soundcraft://commands` (`:219-222`). No prompts.

### 5. Control channel

- **Transport:** NDJSON over 127.0.0.1 (`apps/soundcraft/src/control_server.rs:26-43`). Lines up to about 86 MiB (sized for plugin state), at most 16 connections. No authentication.
- **Methods** (`crates/ui-egui/src/control.rs:98-235`): `engine.execute` / `command`, `engine.commands`, `engine.parity`, `session.inspect`, `ui.inspect`, `ui.set`, `ui.menu.list/invoke`, `ui.click/drag/scroll/zoom/move/key/text`, `ui.screenshot`, `app.quit`.
- **Can target the running GUI:** yes. However, `engine.execute session.open` goes straight to the command (`app.run`) and skips the save prompt the desktop path uses, so it may discard unsaved work.

### 6. Library crates (not published; xtask flags the STANDALONE crates as publishable on their own)

| Crate | Layer | Use | Key API |
| --- | --- | --- | --- |
| **`soundcraft-audio-io`** | 0 | **Probe: duration, sample rate, channels, BWF** (depends only on symphonia) | `probe` (`crates/audio-io/src/lib.rs:210`), `AudioInfo` (`:96`), `decode` (`:230`), `encode` (`:260`) |
| `audio-io::peaks` | 0 | **Waveform data** | `Peaks::build` (`peaks.rs:53`), `spans` (`:130`), `rms_spans` (`:144`) |
| `soundcraft-midi`, `soundcraft-video` | 0 | MIDI probe; H.264/ProRes/MJPEG poster frames | — |
| `soundcraft-model` | 2 | Session parsing | `Session::from_json` (`session.rs:536`) |
| `soundcraft-engine` | 4 | Session summaries (heavy: links plugin-host FFI) | `inspect::session` / `session_text` (`inspect.rs:73` / `:111`) |
| `soundcraft-automation` | 5 | Same API in-process or against the GUI | `Headless`, `Remote`, `Backend::call` (`lib.rs:21-91`) |

### 7. Formats

- **Session fields:** `session.rs:300-333`.
- **Templates:** `session.save_template` strips clips and sources, but `session.new` accepts only the built-in `blank` / `demo` templates (`cmd/file.rs:11`, `:36-51`).
- **Other files:** track presets as JSON, `.scgrp` clip groups, MusicXML, an engraved score SVG.
- **FilmCraft:** none. AAF/OMF are listed as an alpha blocker (`ROADMAP.md:107`). Stems through `file.bounce_stems` are the workable path today.

### 8. Gaps and opportunities (owner: SoundCraft)

| Need | Today | Suggested fix |
| --- | --- | --- |
| Open in a running instance | `app --port P session.open`, only if launched with `--control` | Single-instance forwarder, macOS document-open handling, and route control-channel opens through the save prompt |
| Waveform thumbnail | Only `ui.screenshot` of the window | Add `soundcraft-cli waveform IN OUT.png --width --height`, built on `peaks::spans` |
| Metadata probe | `info` (JSON for audio), `audio_io::probe` | Add `info --json` for `.scraft`; add a header-only `probe_reader(Read+Seek)` |
| Batch export | `run --bounce`, `file.bounce_stems`, `convert` | Optional `convert --glob` |
| Apply a template | Built-in templates only | Accept `template: <path.scraft>` in `session.new` |
| Port 7979 | Shared with VectorCraft's default | Use `--control 0` and read the printed port |


Possible addition to memory: SoundCraft (storytold/soundcraft) is a Pro Tools-style DAW that its siblings' READMEs don't yet list as a Crafting App. For AssetCraft, VectorCraft thumbnails go through `file.place.info`, EffectCraft posters through `render-frame` / `host::session()`, and SoundCraft probes through `soundcraft-audio-io`.
agentId: a5e8ea5ea1339b1a6 (use SendMessage with to: 'a5e8ea5ea1339b1a6', summary: '<5-10 word recap>' to continue this agent)
<usage>subagent_tokens: 427110
tool_uses: 132
duration_ms: 1173862</usage>