# Crafting Apps integration surfaces for AssetCraft: PdfCraft, WordCraft, DesignCraft

Read-only inventory (source read, nothing built or modified) of every surface an external Rust app, AssetCraft (a Bridge-style asset browser), can use to drive the storytold Crafting Apps. Each claim cites `file:line`, relative to that app's repository root. Read at the latest release tags (2026-10-15).

## At a glance

| Surface | PdfCraft 0.8.0 | WordCraft 0.7.0 | DesignCraft 0.8.0 |
| --- | --- | --- | --- |
| Command registry | 194 `CommandSpec`s (static table) | 617 `CommandSpec`s (engine) + ~30 `ui.*` app commands | 459 `cmd!` specs (engine) + 106 UI commands |
| Headless tool table | 163 JSON-Schema tools (`pdfcraft-automation`); 64 registry ids have a tool | every command, through `execute` | every engine command, through `execute` |
| MCP | stdio; 163 tools (+2 meta tools in `--compact`); resources | stdio; 16 tools; 2 resources; headless or `--connect` to the app | stdio; 24 tools; 2 resources; headless or `--connect` to the app |
| Control channel to the GUI | opt-in `--control FILE`; random loopback port; JSON-RPC 2.0 lines; token in an owner-only file | opt-in `--control PORT`; JSON lines; 256-bit key in a 0600 file, sent on every request or once with `auth` | opt-in `--control PORT`; JSON lines; **no authentication** |
| Headless script mode | `run --script steps.json` (JSON array) | none: repeated `run --cmd id={json}` | `script FILE.jsonl` (JSON lines, `$N.path` references), headless or `--connect` |
| Single instance / hand-off | Windows only: a files-only launch hands its files to the running app (UDS) | none | none (one process holds many documents) |
| Library crates without UI deps | 30 crates (`pdfcraft-render`, `-cos`, `-model`, `-organize`, `-create`, …) | 15 (`wordcraft-engine`, `-docx`, `-layout`, `-render`, …) | 15 (`designcraft-format`, `-idml`, `-render`, `-engine`, …) |
| Crate versions in the workspace | `version = "0.8.0"` on every path dep (Cargo.toml:20-50) | path only, no version (Cargo.toml:15-30) | path only, no version (Cargo.toml:16-31) |
| License | MIT OR Apache-2.0 (Cargo.toml:10) | MIT OR Apache-2.0 | MIT OR Apache-2.0 |

---

## PdfCraft (v0.8.0, 9931329)

Workspace `version = "0.8.0"`, `license = "MIT OR Apache-2.0"` (Cargo.toml:8-10). Layering is in docs/architecture.md:30-36: L0 `geom`/`filters`/`crypt`, L1 `cos`, L2 `content`/`model`/`fonts`, L3 `render`/`annot`/`forms`/`js`/`xfa`, L4 feature crates, L6 `engine` façade, L7 `platform`/`ui-egui`/`automation`.

### 1. Desktop app launch (`apps/pdfcraft/src/main.rs`)

Usage: `pdfcraft [options] [files…]` (main.rs:3). Argument parsing is at main.rs:179-200.

| Arg / env | Effect | src |
| --- | --- | --- |
| `files…` (positional) | Opened as tabs. `file://` URIs are decoded to paths (main.rs:105-128). Unquoted paths split by spaces are rejoined (main.rs:134-157, 201). | main.rs:198, 201 |
| `--version` | Prints `pdfcraft <version>` and exits. | main.rs:187-190 |
| `--control <file>` | Turns on the UI control channel: random loopback port, and `{"port","token","pid"}` is written to `<file>` (owner-only, atomic rename). The app refuses to start if it can't publish. | main.rs:16-19, 191, 373-375, 432-460 |
| `--create-images [images…]` | Stages the images into one PDF and asks for the page DPI. | main.rs:4, 192 |
| `--new-window` | Opens its own window instead of handing off. | main.rs:13-14, 193 |
| Any other `--flag value` | A view option applied after the files open (the seed of the control channel's `ui.set`): `--page N --zoom 150 --layout single\|continuous\|two-up\|two-page --continuous on\|off --panel comments\|bookmarks\|pages\|fields\|layers\|attachments\|none --theme --language --mode all\|read\|edit\|convert\|sign --tool <catalogue id> --left open\|closed --organize on --fields on --dialog properties\|shortcuts\|about --palette <query> --home on --cover --default-layout --default-continuous --default-zoom --combine-view grid\|list --combine-zoom 60..200` | main.rs:6-11, 194-197 |
| `PDFCRAFT_RENDERER=wgpu\|gl` | GPU backend choice, with an automatic GL fallback. | main.rs:225, 268-287 |
| `PDFCRAFT_TITLEBAR=app\|system` | Linux window decorations. | src/decorations.rs:9-15 |
| `RUST_LOG`, `WGPU_POWER_PREF`, `WGPU_BACKEND` | Logging and wgpu tuning. | src/logging.rs:251; main.rs:542, 569 |

**Single instance and hand-off.** On Windows, a launch that names only files (no options, no `--control`, no `--create-images`, no `--new-window`) hands them to the PdfCraft that is already running and exits (main.rs:216-224). The transport is a Unix domain socket `%LOCALAPPDATA%\PdfCraft\instance-<SESSIONNAME>.sock`, and the exchange is one JSON line each way: request `{"open":[abs paths]}\n`, reply `ok` (src/single_instance.rs:1-20, 97-113, 140-160). Relative paths are made absolute against the launching process (single_instance.rs:117-119). On other platforms `claim()` returns `Alone` (single_instance.rs:131-135), so each launch opens a new window. On macOS, Finder and `open -a PdfCraft f.pdf` deliver `odoc` Apple events to the running app instead (src/apple_events.rs:1-11).

### 2. CLI (`apps/pdfcraft-cli`)

The usage text is at apps/pdfcraft-cli/src/main.rs:49-74 and dispatch at main.rs:83-106. Exit codes: `0` on success, and also when stdout is a broken pipe; `1` (`ExitCode::FAILURE`) with `pdfcraft-cli: <msg>` on stderr for any error (main.rs:107-114).

| Subcommand | Args | Output | src |
| --- | --- | --- | --- |
| `info <file.pdf> [--password PW]` | | Pretty JSON: `file, pdf_version, file_size, pages, first_page_pt [w,h], title, author, producer, creator, encrypted, tagged, javascript, xfa, xfa_layout, bookmarks, annotations, fields, links, layers, attachments, page_labels (≤8), warnings` | main.rs:281-313 |
| `render <file.pdf> --page N [--dpi 96] --out x.png\|.jpg\|.tif\|.pam [--password]` | | Writes the image; summary on stderr | main.rs:527-570 |
| `text <file.pdf> [--page N]` | | Plain text, pages separated by form feeds | main.rs:53, 315 |
| `edit <in> --out <out> [--rotate 1,3:90] [--delete 2,4] [--move 5:1] [--insert-blank 1] [--title T] [--author A] [--full]` | Options run in order | File | main.rs:54-56, 392 |
| `combine <a.pdf> <b.pdf> … --out combined.pdf` | ≥2 inputs | File (via `Session::combine`) | main.rs:479-489 |
| `extract <in.pdf> --pages 1,3,5 --out out.pdf` | | File | main.rs:491-500 |
| `split <in.pdf> (--every N \| --before 3,7) [--out-dir DIR]` | | Files | main.rs:502 |
| `check <files or dirs…> [--timeout 20] [--dpi 36] [--json out.json]` | Robustness harness; each file runs in a child process (`check-one`) | JSON | main.rs:14-16, 572, 665 |
| `audit <file.pdf> [--password]` | Finds faux redactions | JSON line `{file, findings[], count}`; exit 1 when it finds any | main.rs:160-179 |
| `batch <files or dirs> --steps convert,ocr,optimize --out-dir DIR [--recursive] [--root DIR] [--stop-on-error] [--ppi] [--quality] [--dpi] [--language] [--ocr-all-pages]` | Wraps the `doc_batch` tool | JSON report; exit 1 if any file failed | main.rs:62-64; src/batch.rs:20-60 |
| `tools` | | JSON array of every automation tool: `name, description, read_only, command, input_schema` | main.rs:740-748 |
| `run <tool> [key=value …] [--root DIR] [--out image.png]` | Values are parsed as JSON when they can be | Tool JSON on stdout; images go to `--out` | main.rs:66, 770-796 |
| `run --script steps.json [--root DIR]` | A **JSON array** `[{"tool","args","out"?}]` (not JSONL), run in one session, so doc ids are 1, 2, … | One JSON document per step; stops at the first error with `step N (tool): …` | main.rs:5-9, 67, 773-786 |
| `mcp [--root DIR] [--compact]` | MCP over stdio (needs the `mcp` feature, on by default) | | main.rs:31-46, 798-812; Cargo.toml (cli):34-36 |
| `ui --control FILE <method> [key=value …] [--out shot.png]` | One request to a running GUI | Pretty JSON of `result`; `png_base64` written to `--out` | main.rs:71-72, 814-868 |
| `--version` | | | main.rs:104, 150 |

`--root DIR` confines every path a tool or step names, including `"out"` (main.rs:8-9, 732-738).

### 3. Command registry

There are two tables:

- **`pdfcraft_engine::commands::COMMANDS`**, the menu and palette registry (194 entries; crates/engine/src/commands.rs:163-372). Each `CommandSpec` has `id, label, menu, shortcut, needs, in_text, icon` (commands.rs:118-129), with lookup `command(id)` (commands.rs:373). It holds **no parameters and no run function**: the frontend implements each id (commands.rs:1-10). In the GUI, `ui.command {id}` runs one with no parameters (crates/ui-egui/src/control.rs:288, 733-740).
- **`pdfcraft_automation::tools()`**, the headless API (163 tools with JSON Schemas; crates/automation/src/tools.rs:117-1263). `ToolDef {name,title,description,input_schema,read_only,destructive,command}` (tools.rs:11-23), where `command` links a tool to a registry id. `command_run {id, params}` runs a registry id through its tool and fails with "has no headless tool" otherwise (crates/automation/src/conventions.rs:11-13). 64 of the 194 registry ids have a tool.

Namespaces (registry): a11y 6, actions 3, app 1, bookmark 2, comment 26, create 6, doc 1, edit 22, export 7, file 10, form 21, help 7, measure 7, ocr 3, optimize 2, page 18, print 1, protect 5, redact 7, sign 17, standards 1, tools 2, ui 1, view 18.

#### 3a. Registry commands (all 194)

| id | label | menu | shortcut | needs | headless tool(s) | src |
| --- | --- | --- | --- | --- | --- | --- |
| `file.open` | Open… | FILE | cmd(O) | Nothing | `doc_open` | commands.rs:164 |
| `file.open_recent` | Open Recent | FILE |   | Nothing |   | commands.rs:165 |
| `file.clear_recent` | Clear Recent Files |   |   | Nothing |   | commands.rs:167 |
| `file.pin_folder` | Pin folder to Home… | FILE |   | Nothing |   | commands.rs:168 |
| `create.blank` | New blank PDF | FILE |   | Nothing |   | commands.rs:169 |
| `measure.distance` | Measure distance |   |   | Annotate | `measure_distance` | commands.rs:170 |
| `measure.perimeter` | Measure perimeter |   |   | Annotate | `measure_perimeter` | commands.rs:171 |
| `measure.area` | Measure area |   |   | Annotate | `measure_area` | commands.rs:172 |
| `measure.scale` | Set measurement scale |   |   | Annotate | `measure_scale` | commands.rs:173 |
| `measure.info` | Measurement information |   |   | Document | `measure_info` | commands.rs:174 |
| `measure.snap` | Measurement snapping |   |   | Document | `measure_snap` | commands.rs:175 |
| `measure.export` | Export measurements as CSV |   |   | Document | `measure_export` | commands.rs:176 |
| `page.copy` | Copy pages |   |   | Document |   | commands.rs:177 |
| `page.cut` | Cut pages |   |   | Assembly |   | commands.rs:178 |
| `page.paste` | Paste pages |   |   | Assembly |   | commands.rs:179 |
| `create.file` | Create PDF from file… | FILE |   | Nothing |   | commands.rs:180 |
| `create.multiple` | Create PDF from multiple files… | FILE |   | Nothing | `doc_create_multiple` | commands.rs:181 |
| `create.multiple_separate` | Create separate PDFs… | FILE |   | Nothing |   | commands.rs:182 |
| `create.images` | Create PDF from images… | FILE |   | Nothing |   | commands.rs:183 |
| `create.clipboard` | Create PDF from clipboard | FILE |   | Nothing |   | commands.rs:184 |
| `page.combine` | Combine files… | FILE |   | Nothing | `doc_combine` | commands.rs:185 |
| `file.save` | Save | FILE | cmd(S) | Document | `doc_save` | commands.rs:186 |
| `file.save_as` | Save as… | FILE | cmd_shift(S) | Document |   | commands.rs:187 |
| `file.close` | Close file | FILE | cmd(W) | Document | `doc_close` | commands.rs:188 |
| `file.close_all` | Close all | FILE | cmd_shift(W) | Document |   | commands.rs:189 |
| `file.revert` | Revert | FILE |   | Undo |   | commands.rs:190 |
| `print.dialog` | Print… | FILE | cmd(P) | Document |   | commands.rs:191 |
| `file.properties` | Document properties… | FILE | cmd(D) | Document | `doc_info` | commands.rs:192 |
| `edit.undo` | Undo | EDIT | cmd(Z) | Undo | `edit_undo` | commands.rs:193 |
| `edit.redo` | Redo | EDIT | cmd_shift(Z) | Redo | `edit_redo` | commands.rs:194 |
| `edit.find` | Find… | EDIT | cmd(F) | Document | `text_find` | commands.rs:195 |
| `edit.advanced_search` | Advanced search… | EDIT | cmd_shift(F) | Document |   | commands.rs:196 |
| `view.palette` | Find tools and commands… | VIEW | cmd(K) | Nothing |   | commands.rs:197 |
| `view.layout.single` | Single page |   |   | Document |   | commands.rs:201 |
| `view.layout.two_up` | Two-page view |   |   | Document |   | commands.rs:202 |
| `view.continuous` | Continuous scrolling |   |   | Document |   | commands.rs:203 |
| `view.layout.cover` | Show cover page in two-page view |   |   | TwoPageView |   | commands.rs:204 |
| `view.fit_width_scrolling` | Fit to width scrolling |   |   | Document |   | commands.rs:206 |
| `view.fit_one_page` | Fit one full page |   |   | Document |   | commands.rs:207 |
| `view.fit_visible` | Fit visible |   | cmd(3) | Document |   | commands.rs:210 |
| `view.marquee_zoom` | Marquee zoom | VIEW |   | Document |   | commands.rs:211 |
| `edit.column_select` | Column select | EDIT |   | Document | `text_extract` | commands.rs:215 |
| `edit.snapshot` | Take a snapshot | EDIT |   | Copying |   | commands.rs:216 |
| `view.full_screen` | Full screen mode | VIEW | cmd(L) | Document |   | commands.rs:217 |
| `view.read_mode` | Read mode | VIEW | Some(Shortcut { command: true, shift: false, mac_ctrl: true, key: "H" }) | Document |   | commands.rs:218 |
| `view.next_tab` | Next tab | VIEW | Some(Shortcut { command: false, shift: false, mac_ctrl: true, key: "Tab" }) | Nothing |   | commands.rs:222 |
| `view.previous_tab` | Previous tab | VIEW | Some(Shortcut { command: false, shift: true, mac_ctrl: true, key: "Tab" }) | Nothing |   | commands.rs:223 |
| `view.focus_page_input` | Go to page… | VIEW | cmd_shift(N) | Document |   | commands.rs:224 |
| `ui.dock.reset` | Reset panel layout | VIEW |   | Nothing |   | commands.rs:225 |
| `view.theme` | Switch light / dark theme |   |   | Nothing |   | commands.rs:226 |
| `view.theme.system` | Use system setting |   |   | Nothing |   | commands.rs:227 |
| `view.theme.light` | Light gray |   |   | Nothing |   | commands.rs:228 |
| `view.theme.dark` | Dark gray |   |   | Nothing |   | commands.rs:229 |
| `comment.list` | Comments panel | VIEW |   | Document |   | commands.rs:230 |
| `comment.note` | Add a sticky note |   |   | Annotate |   | commands.rs:231 |
| `comment.freetext` | Add a text box |   |   | Annotate |   | commands.rs:232 |
| `comment.highlight` | Highlight text |   |   | Annotate |   | commands.rs:233 |
| `comment.underline` | Underline text |   |   | Annotate |   | commands.rs:234 |
| `comment.strikeout` | Strikethrough text |   |   | Annotate |   | commands.rs:235 |
| `comment.squiggly` | Squiggly underline text |   |   | Annotate |   | commands.rs:236 |
| `comment.ink` | Draw freehand |   |   | Annotate |   | commands.rs:237 |
| `comment.line` | Draw a line |   |   | Annotate |   | commands.rs:238 |
| `comment.arrow` | Draw an arrow |   |   | Annotate |   | commands.rs:239 |
| `comment.square` | Draw a rectangle |   |   | Annotate |   | commands.rs:240 |
| `comment.circle` | Draw an oval |   |   | Annotate |   | commands.rs:241 |
| `comment.polygon` | Draw a polygon |   |   | Annotate |   | commands.rs:242 |
| `comment.polyline` | Draw connected lines |   |   | Annotate |   | commands.rs:243 |
| `comment.cloud` | Draw a cloud |   |   | Annotate |   | commands.rs:244 |
| `comment.callout` | Add a callout |   |   | Annotate |   | commands.rs:245 |
| `comment.caret` | Insert text |   |   | Annotate |   | commands.rs:246 |
| `comment.replace` | Replace text |   |   | Annotate |   | commands.rs:247 |
| `comment.attach` | Attach a file |   |   | Annotate |   | commands.rs:248 |
| `comment.eraser` | Erase drawings |   |   | Annotate |   | commands.rs:249 |
| `form.fields` | Form fields panel | VIEW |   | Document |   | commands.rs:250 |
| `form.clear` | Clear form | EDIT |   | FillForms |   | commands.rs:251 |
| `comment.flatten` | Flatten comments |   |   | HasComments |   | commands.rs:252 |
| `comment.import` | Import comments… |   |   | Modification |   | commands.rs:253 |
| `comment.stamp` | Add a stamp |   |   | Annotate | `stamp_custom` | commands.rs:254 |
| `comment.export` | Export all comments to data file… |   |   | HasComments |   | commands.rs:255 |
| `comment.hide_all` | Hide all comments |   |   | Document | `comments_hide` | commands.rs:256 |
| `comment.summarize` | Summarize comments |   |   | HasComments | `comments_summarize` | commands.rs:257 |
| `form.import_data` | Import form data… |   |   | HasFields |   | commands.rs:258 |
| `form.export_data` | Export form data… |   |   | HasFields |   | commands.rs:259 |
| `standards.pdfa` | PDF/A… |   |   | Document | `pdfa_verify`, `pdfa_convert` | commands.rs:260 |
| `actions.wizard` | Action Wizard… |   |   | Nothing | `action_list`, `action_run` | commands.rs:261 |
| `actions.distribution` | Prepare for distribution… |   |   | Nothing |   | commands.rs:262 |
| `actions.optimize_scans` | Optimize scanned documents… |   |   | Nothing |   | commands.rs:263 |
| `doc.compare` | Compare files… |   |   | Document | `doc_compare`, `doc_compare_report`, `doc_compare_mark` | commands.rs:264 |
| `form.detect` | Detect form fields |   |   | Modification | `form_detect_fields` | commands.rs:265 |
| `form.merge_data` | Merge data files into spreadsheet… |   |   | Nothing | `form_merge_data` | commands.rs:266 |
| `form.flatten` | Flatten form fields |   |   | HasFields |   | commands.rs:267 |
| `form.prepare` | Prepare a form |   |   | Modification | `form_set_script`, `form_actions`, `form_set_actions` | commands.rs:268 |
| `form.tab_order.row` | Tab order: by rows |   |   | HasFields |   | commands.rs:269 |
| `form.tab_order.column` | Tab order: by columns |   |   | HasFields |   | commands.rs:270 |
| `form.tab_order.structure` | Tab order: by document structure |   |   | HasFields |   | commands.rs:271 |
| `edit.text` | Add text |   |   | Modification | `page_add_text` | commands.rs:272 |
| `edit.edit_text` | Edit text & images |   |   | Modification | `text_lines`, `image_hit`, `page_images`, `image_edit`, `image_save`, `text_font_files`, `text_font_import`, `text_fonts`, `text_paragraphs`, `text_edit_layout`, `text_edit` | commands.rs:273 |
| `edit.link` | Add or edit links |   |   | Modification | `link_add` | commands.rs:274 |
| `edit.links_from_urls` | Create links from URLs |   |   | Modification | `links_from_urls` | commands.rs:275 |
| `edit.remove_links` | Remove all links |   |   | Modification | `links_remove` | commands.rs:276 |
| `edit.bates` | Add Bates numbering… |   |   | Modification |   | commands.rs:277 |
| `edit.image` | Add image… |   |   | Modification | `page_add_image` | commands.rs:278 |
| `redact.mark` | Redact text and images |   |   | Modification |   | commands.rs:279 |
| `redact.pages` | Redact pages… |   |   | Modification |   | commands.rs:280 |
| `redact.search` | Find text and redact… |   |   | Modification |   | commands.rs:281 |
| `redact.properties` | Redaction properties… |   |   | Document |   | commands.rs:282 |
| `redact.apply` | Apply redactions… |   |   | HasRedactions |   | commands.rs:283 |
| `redact.clear` | Clear redaction marks |   |   | HasRedactions |   | commands.rs:284 |
| `protect.remove_hidden` | Remove hidden information… |   |   | Modification |   | commands.rs:285 |
| `redact.sanitize` | Sanitize document… |   |   | Modification |   | commands.rs:286 |
| `form.field.properties` | Field properties… |   |   | HasFields |   | commands.rs:287 |
| `form.add.text` | Add a text field |   |   | Modification |   | commands.rs:288 |
| `form.add.checkbox` | Add a checkbox |   |   | Modification |   | commands.rs:289 |
| `form.add.radio` | Add a radio button |   |   | Modification |   | commands.rs:290 |
| `form.add.combo` | Add a drop-down list |   |   | Modification |   | commands.rs:291 |
| `form.add.list` | Add a list box |   |   | Modification |   | commands.rs:292 |
| `form.add.button` | Add a button |   |   | Modification |   | commands.rs:293 |
| `form.add.image` | Add an image field |   |   | Modification |   | commands.rs:294 |
| `form.add.date` | Add a date field |   |   | Modification |   | commands.rs:295 |
| `form.add.signature` | Add a digital signature field |   |   | Modification |   | commands.rs:296 |
| `sign.digital` | Digitally sign |   |   | Document | `sign_windows_ids`, `sign_keychain_ids`, `sign_document` | commands.rs:297 |
| `sign.certify` | Certify (visible signature) |   |   | Document |   | commands.rs:298 |
| `sign.certify_invisible` | Certify (invisible signature) |   |   | Document |   | commands.rs:299 |
| `sign.validate` | Validate all signatures |   |   | Document |   | commands.rs:300 |
| `sign.panel` | Signatures panel | VIEW |   | Document |   | commands.rs:301 |
| `sign.fill.text` | Fill & Sign: add text |   |   | Annotate | `fill_sign_add` | commands.rs:302 |
| `sign.fill.check` | Fill & Sign: checkmark |   |   | Annotate |   | commands.rs:303 |
| `sign.fill.cross` | Fill & Sign: cross |   |   | Annotate |   | commands.rs:304 |
| `sign.fill.dot` | Fill & Sign: dot |   |   | Annotate |   | commands.rs:305 |
| `sign.fill.line` | Fill & Sign: line |   |   | Annotate |   | commands.rs:306 |
| `sign.fill.date` | Fill & Sign: date |   |   | Annotate |   | commands.rs:307 |
| `sign.fill.signature` | Fill & Sign: sign |   |   | Annotate |   | commands.rs:308 |
| `sign.fill.initials` | Fill & Sign: initials |   |   | Annotate |   | commands.rs:309 |
| `sign.fill.signature.change` | Fill & Sign: change signature |   |   | Nothing |   | commands.rs:310 |
| `sign.fill.signature.remove` | Fill & Sign: remove saved signature |   |   | Nothing |   | commands.rs:311 |
| `sign.fill.initials.remove` | Fill & Sign: remove saved initials |   |   | Nothing |   | commands.rs:312 |
| `sign.fill.initials.change` | Fill & Sign: change initials |   |   | Nothing |   | commands.rs:313 |
| `export.image` | Export to image… | FILE |   | Copying | `doc_export_images` | commands.rs:314 |
| `optimize.reduce` | Reduce file size… | FILE |   | Document | `doc_reduce` | commands.rs:315 |
| `optimize.advanced` | Optimize PDF… | FILE |   | Document | `doc_audit_space`, `doc_optimize` | commands.rs:316 |
| `export.text` | Export to text… | FILE |   | Copying | `doc_export_text` | commands.rs:317 |
| `export.docx` | Export to Word… | FILE |   | Copying | `doc_export_office` | commands.rs:318 |
| `export.pptx` | Export to PowerPoint… | FILE |   | Copying |   | commands.rs:319 |
| `export.html` | Export to HTML… | FILE |   | Copying |   | commands.rs:320 |
| `export.rtf` | Export to RTF… | FILE |   | Copying |   | commands.rs:321 |
| `app.preferences` | Preferences… | EDIT | cmd(,) | Nothing |   | commands.rs:322 |
| `tools.js_console` | JavaScript console… |   | cmd(J) | Document | `js_run` | commands.rs:323 |
| `tools.document_js` | Document JavaScripts… |   |   | Modification | `js_document_scripts`, `js_set_document_script` | commands.rs:324 |
| `ocr.recognize` | Recognize text… |   |   | Modification | `ocr_recognize`, `ocr_status` | commands.rs:325 |
| `ocr.enhance_camera` | Enhance camera image |   |   | Modification | `page_enhance_camera` | commands.rs:326 |
| `ocr.recognize_batch` | Recognize text in multiple files… |   |   | Nothing | `ocr_recognize_files` | commands.rs:327 |
| `a11y.check` | Check for accessibility… |   |   | Document | `accessibility_check` | commands.rs:328 |
| `a11y.autotag` | Automatically tag PDF |   |   | Document |   | commands.rs:329 |
| `a11y.report` | Open accessibility report |   |   | Document | `accessibility_report` | commands.rs:330 |
| `a11y.reading_options` | Change reading options… |   |   | Document | `view_reading_colors` | commands.rs:331 |
| `a11y.alt_text` | Add alternate text… |   |   | Modification | `accessibility_figures`, `accessibility_set_alt` | commands.rs:332 |
| `a11y.tags` | Tags panel |   |   | Document | `accessibility_tags` | commands.rs:333 |
| `export.all_images` | Export all images… | FILE |   | Copying | `doc_export_all_images` | commands.rs:334 |
| `edit.header_footer` | Add header & footer… |   |   | Modification | `doc_header_footer` | commands.rs:335 |
| `edit.header_footer.update` | Update header & footer… |   |   | Marks(crate::MarkKind::HeaderFooter) |   | commands.rs:336 |
| `edit.header_footer.remove` | Remove header & footer |   |   | Marks(crate::MarkKind::HeaderFooter) |   | commands.rs:337 |
| `edit.watermark` | Add watermark… |   |   | Modification | `doc_watermark` | commands.rs:338 |
| `edit.watermark.update` | Update watermark… |   |   | Marks(crate::MarkKind::Watermark) |   | commands.rs:339 |
| `edit.watermark.remove` | Remove watermark |   |   | Marks(crate::MarkKind::Watermark) |   | commands.rs:340 |
| `edit.background` | Add background… |   |   | Modification | `doc_background` | commands.rs:341 |
| `edit.background.update` | Update background… |   |   | Marks(crate::MarkKind::Background) |   | commands.rs:342 |
| `edit.background.remove` | Remove background |   |   | Marks(crate::MarkKind::Background) |   | commands.rs:343 |
| `protect.password` | Protect using password… | FILE |   | Security | `doc_protect` | commands.rs:344 |
| `protect.remove` | Remove security | FILE |   | ProtectedSecurity | `doc_unprotect` | commands.rs:345 |
| `protect.unlock` | Enter permissions password… |   |   | Unlock |   | commands.rs:346 |
| `protect.properties` | Security properties… | FILE |   | Document |   | commands.rs:347 |
| `page.organize` | Organize pages | PAGES |   | Document |   | commands.rs:348 |
| `bookmark.add` | New bookmark | PAGES | cmd(B) | Assembly |   | commands.rs:349 |
| `bookmark.from_structure` | New bookmarks from structure | PAGES |   | Assembly |   | commands.rs:350 |
| `page.rotate` | Rotate pages clockwise | PAGES |   | Assembly | `page_rotate` | commands.rs:351 |
| `page.rotate_ccw` | Rotate pages counterclockwise | PAGES |   | Assembly |   | commands.rs:352 |
| `page.delete` | Delete pages | PAGES |   | Assembly | `page_delete` | commands.rs:353 |
| `page.insert_blank` | Insert blank page | PAGES |   | Assembly | `page_insert_blank` | commands.rs:354 |
| `page.rotate_dialog` | Rotate pages… | PAGES |   | Assembly |   | commands.rs:355 |
| `page.duplicate` | Duplicate pages | PAGES |   | Assembly | `page_duplicate` | commands.rs:356 |
| `page.crop` | Crop pages | PAGES |   | Assembly |   | commands.rs:357 |
| `page.boxes` | Set page boxes… | PAGES |   | Assembly | `page_set_box` | commands.rs:358 |
| `page.insert` | Insert pages from file… | PAGES |   | Assembly | `page_insert_file` | commands.rs:359 |
| `page.replace` | Replace pages… | PAGES |   | Assembly | `page_replace` | commands.rs:360 |
| `page.extract` | Extract pages… | PAGES |   | Assembly | `page_extract` | commands.rs:361 |
| `page.split` | Split document… | PAGES |   | Assembly | `doc_split` | commands.rs:362 |
| `page.number` | Number pages… | PAGES |   | Assembly |   | commands.rs:363 |
| `help.shortcuts` | Keyboard shortcuts | HELP |   | Nothing |   | commands.rs:364 |
| `help.discord` | Join the ArtCraft Discord | HELP |   | Nothing |   | commands.rs:365 |
| `help.app_page` | PdfCraft web page | HELP |   | Nothing |   | commands.rs:366 |
| `help.github` | PdfCraft on GitHub | HELP |   | Nothing |   | commands.rs:367 |
| `help.website` | ArtCraft website | HELP |   | Nothing |   | commands.rs:368 |
| `help.check_updates` | Check for updates… | HELP |   | Nothing |   | commands.rs:369 |
| `help.about` | About PdfCraft | HELP |   | Nothing |   | commands.rs:370 |

#### 3b. Automation tools (all 163; the headless API, also the MCP tool list)

`flags`: `ro` = read-only, `destructive` = may remove content or overwrite files. The `params` column lists the top-level schema properties found in the source; for exact types, run `pdfcraft-cli tools`.

| tool | flags | registry cmd | params | purpose | src |
| --- | --- | --- | --- | --- | --- |
| `doc_batch` |   |   | paths,out_dir,steps,recursive,stop_on_error,ppi,quality,dpi,language,skip_text_pages | Process files and folders — Run convert, ocr and/or optimize over files and folders, one file at a time | tools.rs:121 |
| `doc_open` |   | `file.open` | path,password | Open a PDF — Open a PDF file and return its document id, page count and whether it can be edited. | tools.rs:132 |
| `doc_list` | ro |   |   | List open documents — List the open documents with their ids, page counts and unsaved state. | tools.rs:135 |
| `doc_info` | ro | `file.properties` | doc | Inspect a document — Metadata, page sizes and labels, bookmarks, annotations, form fields, links, layers, attachments, fonts, security and repair notes. | tools.rs:136 |
| `doc_close` |   | `file.close` | doc,discard_changes | Close a document — Close a document | tools.rs:140 |
| `doc_save` | destructive | `file.save` | doc,path,full,flatten_fill_sign | Save a document — Save to its own file (an incremental update, which keeps signatures valid) or to a new path (a full rewrite) | tools.rs:143 |
| `doc_set_info` |   |   | doc,key,value | Set document metadata — Set a document information entry such as Title, Author, Subject or Keywords; an empty value removes it | tools.rs:159 |
| `page_render` | ro |   | doc,page,dpi,hide_text_block | Render a page — Render one page to a PNG image (default 96 dpi, at most 600). | tools.rs:161 |
| `object_list` | ro |   | doc,page | List editable objects — List existing paragraphs, Image/Form artwork and added content once, with source kind/index references, displayed rectangles and document generation | tools.rs:166 |
| `object_move` |   |   | doc,page,objects,kind,index,offset,generation | Move objects together — Atomically translate 1–1000 mixed references from object_list by [dx, dy] in displayed points (right/down) | tools.rs:168 |
| `text_extract` | ro | `edit.column_select` | doc,pages,rect | Extract text — Extract the text of some or all pages, in reading order | tools.rs:174 |
| `text_find` | ro | `edit.find` | doc,query,limit,start_page,skip | Find text — Find a phrase (case-insensitive, whitespace-normalised) and return matches in page order with page and line rectangles in points (origin top-left) | tools.rs:184 |
| `page_rotate` |   | `page.rotate` | doc,pages,degrees,subset,orientation | Rotate pages — Rotate pages by a multiple of 90 degrees (positive is clockwise): the listed pages (default all), filtered like Acrobat's Rotate Pages by subset (all, | tools.rs:190 |
| `page_enhance_camera` |   | `ocr.enhance_camera` | doc,page,corners,level,mode | Enhance camera image — Straighten a photographed document (Scan & OCR ▸ Enhance ▸ Camera image): the part of the page inside the four corners (x1, y1, … x4, y4 in top-left-o | tools.rs:200 |
| `page_delete` | destructive | `page.delete` | doc,pages | Delete pages — Delete pages | tools.rs:212 |
| `page_move` |   |   | doc,pages,to | Move pages — Move pages so the first of them lands at position `to` (1-based, counted before the move) | tools.rs:216 |
| `page_insert_blank` |   | `page.insert_blank` | doc,at,width,height | Insert a blank page — Insert a blank page so it becomes page `at` | tools.rs:220 |
| `page_insert_file` |   | `page.insert` | doc,path,at,pages | Insert pages from a file — Insert pages of another PDF so the first becomes page `at` | tools.rs:226 |
| `page_extract` |   | `page.extract` | doc,pages,out,open,separate,out_dir,delete | Extract pages — Copy pages into a new PDF (links, bookmarks, fields and layers that belong to them come along) | tools.rs:229 |
| `doc_combine` |   | `page.combine` | paths,pages,passwords,groups,field_conflicts,out,open | Combine files — Combine files, in order, into one PDF (bookmarks are kept under one entry per file) | tools.rs:235 |
| `doc_create_multiple` |   | `create.multiple` | paths,mode,pages,out,open,out_dir | Create PDF from multiple files — Convert several files (PDFs, PNG/JPEG/JPEG 2000/TIFF/GIF/BMP images, .txt files) to PDF in one run | tools.rs:249 |
| `doc_split` |   | `page.split` | doc,every,before,bookmarks,max_mb,out_dir | Split a document — Split into several files written to out_dir: every N pages, before given pages, at top-level bookmarks (bookmarks: true; files named after them, repea | tools.rs:262 |
| `bookmark_list` | ro |   | doc,offset,limit | List bookmarks — The bookmarks in outline order, one page at a time: each with its path (1-based, as the other bookmark tools take it), title, target page and open sta | tools.rs:268 |
| `bookmark_add` |   |   | doc,title,page,parent,position | Add a bookmark — Add a bookmark that goes to a page, under a parent bookmark (or at the top level), at a position | tools.rs:282 |
| `bookmark_rename` |   |   | doc,path,title | Rename a bookmark — Change a bookmark's title | tools.rs:286 |
| `bookmark_delete` | destructive |   | doc,path | Delete a bookmark — Delete a bookmark and the bookmarks under it | tools.rs:288 |
| `bookmark_move` |   |   | doc,path,parent,position | Move a bookmark — Move a bookmark (with its children) under another parent, or within its level | tools.rs:291 |
| `bookmark_set_page` |   |   | doc,path,page | Set a bookmark's page — Point a bookmark at another page | tools.rs:295 |
| `bookmark_from_structure` |   |   | doc | New bookmarks from structure — Make bookmarks from the document's tagged headings (H, H1-H6), nested by level under a new first bookmark titled \"Untitled\" | tools.rs:297 |
| `page_number` |   |   | doc,from,to,style,prefix,start | Number pages — Label a range of pages (e.g | tools.rs:303 |
| `form_fields` | ro |   | doc | List form fields — Every interactive form field: name, type (text, checkbox, radio, combo, list, button, signature), value, options, page, rect (top-left-origin points) | tools.rs:314 |
| `form_fill` |   |   | doc,values | Fill in form fields — Set several fields at once (one undo step) | tools.rs:317 |
| `form_reset` | destructive |   | doc,fields | Clear form — Reset fields to their default values: all of them, or only those listed | tools.rs:323 |
| `form_add_field` |   |   | doc,page,rect,name,multiline,group,export,options,editable,multi_select,caption | Prepare form: add a field — Add a form field on a page | tools.rs:326 |
| `stamp_custom` |   | `comment.stamp` | doc,page,path,at,file_page,name,author | Add a custom stamp — Stamp a picture: a PDF page (file_page, default 1) or an image file, centred at at [x, y] (points from the top-left of the displayed page) at its natu | tools.rs:348 |
| `form_set_image` |   |   | doc,field,path | Set an image field's picture — Show an image file (PNG, JPEG, TIFF, GIF or BMP) in an image field or button, scaled to fit and centred (what clicking an image field does) | tools.rs:362 |
| `form_set_props` |   |   | doc,field,fields,name,tooltip,read_only,multiline,max_length,options,align,locked,check_style | Field properties — Change field properties: give field for one field, or fields for a list of unique field names (1–1000) | tools.rs:364 |
| `form_tab_order` |   |   | doc,order,pages,field,move | Set the tab order — Set the tab order of pages (default all): row (top to bottom, left to right), column, structure, or annotations (unspecified) | tools.rs:400 |
| `doc_export_data` | destructive |   | doc,path,what | Export comments or form data — Write comments and/or form data to path; the extension picks the format: .xfdf or .fdf (comments and/or fields), .xml, .csv or .txt (form data) | tools.rs:405 |
| `doc_import_data` |   |   | doc,path | Import comments or form data — Import comments and/or field values from an XFDF, FDF, XML, CSV or tab-delimited text file (detected from its content) | tools.rs:408 |
| `form_delete_field` | destructive |   | doc,field | Delete a field — Delete a form field and all its widgets | tools.rs:410 |
| `redact_mark` |   |   | doc,page,rect,find,words,case_sensitive,whole_words,whole_pages,pages,overlay,code_set,codes | Mark for redaction — Mark content for redaction (nothing is removed until redact_apply) | tools.rs:413 |
| `redact_apply` | destructive |   | doc,pages | Apply redactions — Apply the redaction marks (all, or those on pages): text, images, vectors, comments and form fields under them are removed for good and boxes are draw | tools.rs:438 |
| `redact_clear` | destructive |   | doc | Remove redaction marks — Remove every redaction mark without applying it | tools.rs:445 |
| `doc_hidden_info` | ro |   | doc | Find hidden information — What Remove Hidden Information would remove, by category (metadata, attachments, comments, form-fields, hidden-text, hidden-layers, bookmarks, links-a | tools.rs:448 |
| `doc_remove_hidden` | destructive |   | doc,categories | Remove hidden information / Sanitize — Remove the listed categories (see doc_hidden_info), or every category when none are listed (Sanitize Document) | tools.rs:455 |
| `doc_audit_redactions` | ro |   | doc | Audit faux redactions — Find faux redactions: opaque filled rectangles painted over text that is still extractable, and /Redact annotations that were marked but never applied | tools.rs:468 |
| `printers` | ro |   |   | List printers — The printers the system's print spooler knows (CUPS on macOS and Linux), with the default marked. | tools.rs:475 |
| `printer_options` | ro |   | printer | List printer options — A printer driver's own job options (CUPS: from its PPD), such as the paper tray, paper type or finishing: key, label, group, default and choices | tools.rs:476 |
| `doc_print` |   |   | doc,pages,subset,reverse,layout,scale,per_sheet,order,border,auto_rotate,booklet_subset,binding | Print — Print with Acrobat's Print dialog options, or save the print-ready PDF | tools.rs:479 |
| `content_list` | ro |   | doc | List added content — Text and images added with page_add_text/page_add_image (or Edit a PDF ▸ Add content), per page with a 1-based index, rect (points from the top-left o | tools.rs:513 |
| `page_add_text` |   | `edit.text` | doc,page,text,at,width,rect,font,size,bold,italic,color,align | Add text to a page — Add text as page content (not a comment) | tools.rs:516 |
| `page_add_image` |   | `edit.image` | doc,page,path,rect | Add an image to a page — Add an image file (PNG, JPEG, TIFF, GIF, BMP) as page content: in rect (points from the top-left of the page), or centred at its natural size (shrunk | tools.rs:534 |
| `content_update` |   |   | doc,page,index,rect,text,font,size,bold,italic,color,align,rotate | Edit added content — Move/resize (rect), retype (text) or restyle (font, size, bold, italic, color, align) an added item (page, index from content_list) | tools.rs:544 |
| `content_delete` | destructive |   | doc,page,index | Delete added content — Delete an added item (page, index from content_list) | tools.rs:558 |
| `link_list` | ro |   | doc | List links — Every link: page, 1-based index (for link_edit/link_delete), rect (points from the top-left of the page), and where it goes (url or to_page). | tools.rs:561 |
| `link_open` |   |   | doc,page,index,password,allow_absolute | Open a linked PDF — Follow a local GoToR link in PdfCraft: open the referenced PDF as a new document, respecting --root | tools.rs:564 |
| `link_add` |   | `edit.link` | doc,page,rect,url,to_page,visible,color,width,highlight | Add a link — Add a link over rect on page that opens url or goes to to_page | tools.rs:569 |
| `link_edit` |   |   | doc,page,index,rect,url,to_page,visible,color,width,highlight | Edit a link — Change a link's rect, destination (url or to_page) or appearance | tools.rs:579 |
| `link_delete` | destructive |   | doc,page,index | Delete a link — Delete one link (page, index from link_list) | tools.rs:589 |
| `links_from_urls` |   | `edit.links_from_urls` | doc | Create links from URLs — Find web addresses (http://, https://, www.) in the text of every page and make them clickable links | tools.rs:592 |
| `links_remove` | destructive | `edit.remove_links` | doc,pages | Remove all links — Remove every link in the document (or on the given pages) | tools.rs:595 |
| `measure_distance` |   | `measure.distance` |   | Measure distance — Add an undoable two-point distance annotation using the scale of the first point's viewport. | tools.rs:599 |
| `measure_perimeter` |   | `measure.perimeter` |   | Measure perimeter — Add an undoable connected-line length annotation | tools.rs:601 |
| `measure_area` |   | `measure.area` |   | Measure area — Add an undoable area annotation from a simple polygon | tools.rs:603 |
| `measure_info` | ro | `measure.info` | doc,page,points | Read a measurement — Calculate a live distance, perimeter or area, deltas, angle and scale without adding an annotation | tools.rs:605 |
| `measure_list` | ro |   | doc,page | List measurements — Saved measurement annotations with calculated values, scale and vertices in display coordinates | tools.rs:607 |
| `measure_scale` |   | `measure.scale` | doc,page,at,rect,name,unit,precision,units_per_point,points,distance | Set or read a measurement scale — Read the scale at a point, or add a rectangular viewport using units_per_point or two calibration points and their real-world distance | tools.rs:609 |
| `measure_snap` | ro | `measure.snap` | doc,page,at,tolerance,endpoints,midpoints,intersections,paths | Snap a measurement vertex — Snap a point to vector paths, endpoints, midpoints or intersections | tools.rs:611 |
| `measure_export` |   | `measure.export` | doc,out | Export measurements as CSV — Atomically write saved measurement values, labels, authors and scale ratios as spreadsheet-safe CSV | tools.rs:613 |
| `comment_list` | ro |   | doc,page | List comments — Every comment (annotation other than links, form widgets and pop-ups) with its page, index, id, type, author, text, date, rectangle, colour, review st | tools.rs:615 |
| `comment_add` |   |   | doc,page,path,points,knee,contents,author,at,rect,from,to,find | Add a comment — Add a comment as Acrobat's commenting tools do | tools.rs:618 |
| `comment_reply` |   |   | text,author | Reply to a comment — Add a reply to a comment's thread | tools.rs:656 |
| `comment_set_status` |   |   | status,author | Set a comment's status — Set a comment's review status (Acrobat: Set status), recorded as a status reply | tools.rs:660 |
| `sign_list` | ro |   | doc | List and validate signatures — Every signature field with its validation: status (valid, unknown = intact but the signer isn't trusted, invalid, unsigned), signer and certificate, d | tools.rs:664 |
| `sign_id_create` |   |   | name,organization,unit,email,country,key,years,password,path | Create a digital ID — Create a self-signed digital ID and save it as a password-protected .p12 file (Acrobat: Configure a new digital ID ▸ Create a new digital ID ▸ Save to | tools.rs:667 |
| `sign_windows_ids` | ro | `sign.digital` |   | List Windows store digital IDs — Windows: signing identities in the Current User Personal certificate store (certificate details and the windows: reference sign_document takes) | tools.rs:681 |
| `sign_keychain_ids` | ro | `sign.digital` |   | List Keychain digital IDs — macOS: the signing identities in the user's keychains (certificate details and the keychain: reference sign_document takes) | tools.rs:685 |
| `sign_document` |   | `sign.digital` | doc,id,password,field,page,rect,reason,location,contact,certify,graphic,image | Sign a document — Sign with a digital ID (a .p12/.pfx path, or on macOS a Keychain identity: \"keychain:<common name or fingerprint>\" from sign_keychain_ids, or on Win | tools.rs:689 |
| `sign_trust` |   |   | paths,password,clear | Trust certificates — Add certificates (.cer/.crt/.pem/.der, or the certificates in a .p12 with `password`) to the trusted certificates used to validate signatures, or clea | tools.rs:718 |
| `comment_mark` |   |   | marked,author | Mark a comment with a checkmark — Mark or unmark a comment with a checkmark (Acrobat: Mark with checkmark), recorded as a private reply | tools.rs:722 |
| `comment_lock` |   |   | locked | Lock a comment — Lock or unlock a comment (Properties ▸ Locked) | tools.rs:726 |
| `comments_hide` |   | `comment.hide_all` | doc,hidden | Hide all comments — Hide or show every comment on the page (fields and links still draw) | tools.rs:730 |
| `view_reading_colors` |   | `a11y.reading_options` | doc,mode,text,background,only_black,line_art | Set reading colours — Set the document's reading colours (Acrobat's Replace Document Colors, in the Reading Options): solid text and line-art paints are remapped for readab | tools.rs:733 |
| `comments_summarize` |   | `comment.summarize` | doc,sort,out,open | Summarize comments — Make a PDF summarising every comment (number, author, type, date, text, replies), sorted by page, author, date or type. | tools.rs:746 |
| `comment_edit` |   |   | contents,color,opacity,width,fill,rect,move | Edit a comment — Change a comment's text, colour, opacity, line width, fill (rectangle/oval/polygon; \"none\" removes it), rectangle (rectangle/oval/text box/stamp) or | tools.rs:752 |
| `comment_image_preview` | ro |   | layer,dpi | Preview image signature layers — Return a PNG of the page without the selected image signature/initials (layer=background, default), or its embedded image with alpha (layer=image) | tools.rs:764 |
| `comment_delete` | destructive |   |   | Delete a comment — Delete a comment with its pop-up and replies | tools.rs:766 |
| `doc_protect` |   | `protect.password` | doc,open_password,permissions_password,printing,changes,copy,accessibility,compatibility,encrypt_metadata | Protect with passwords — Encrypt the document (applied by the next doc_save, a full rewrite) | tools.rs:767 |
| `doc_unprotect` | destructive | `protect.remove` | doc | Remove security — Remove password security (the document must have been opened with its permissions password, or have none) | tools.rs:787 |
| `page_replace` |   | `page.replace` | doc,pages,path,from_pages | Replace pages — Replace the content of pages with pages of another PDF (same count) | tools.rs:791 |
| `page_duplicate` |   | `page.duplicate` | doc,pages | Duplicate pages — Insert copies of pages after the last of them (fonts and images are shared, not copied) | tools.rs:794 |
| `page_set_box` |   | `page.boxes` | doc,pages,box,margins,rect | Set page boxes / crop — Set a page box (crop by default; also trim, bleed, art, media) on pages: either margins in points from the media box [left, bottom, right, top], or an | tools.rs:797 |
| `doc_header_footer` |   | `edit.header_footer` | doc,pages,header_left,header_center,header_right,footer_left,footer_center,footer_right,font_size,color,margins,start_number | Add header and footer — Add a header and/or footer to pages | tools.rs:813 |
| `doc_watermark` |   | `edit.watermark` | doc,pages,text,file,file_page,scale,font_size,color,opacity,rotation,behind,offset_x | Add watermark — Add a watermark to pages: text, or a picture from `file` (an image, or page `file_page` of a PDF) at `scale` of the page (default 0.5); rotated (degre | tools.rs:833 |
| `doc_background` |   | `edit.background` | doc,pages,color,file,file_page,scale,opacity,replace | Add background — Fill page backgrounds with a colour, or a picture from `file` (an image, or page `file_page` of a PDF) fitted at `scale` (default 1), behind the conte | tools.rs:854 |
| `doc_remove_marks` | destructive |   | doc,kind | Remove header & footer, watermark or background — Remove every header and footer, watermark or background PdfCraft (or a compatible tool) added | tools.rs:860 |
| `doc_export_images` |   | `export.image` | doc,folder,quality,dpi,pages | Export pages as images — Write pages as PNG, JPEG or TIFF files (`<name>_page_<n>.png\|jpg\|tif`) into a folder, at a resolution (default 150 dpi) | tools.rs:863 |
| `accessibility_autotag_preview` | ro |   | doc,page | Preview automatic tags — Propose paragraph and heading tags for visible text blocks on one page | tools.rs:876 |
| `accessibility_autotag_apply` |   |   | doc | Automatically tag text-only PDF — Write marked content, a Document structure element, paragraph tags and a parent tree for every text object in an untagged, text-only PDF | tools.rs:879 |
| `accessibility_check` | ro | `a11y.check` | doc,rules,categories,all,pages | Check for accessibility — Run the Accessibility Checker's full check (32 rules in 7 categories: document, page_content, forms, alternate_text, tables, lists, headings) | tools.rs:881 |
| `accessibility_report` | destructive | `a11y.report` | doc,path,rules,categories,all,pages | Accessibility report — Run the full check and write the accessibility report (HTML) to path; returns the results too | tools.rs:894 |
| `accessibility_review_record` | destructive |   | before,after,path,all | Accessibility review record — Compare the same accessibility rules on two open versions of a PDF and write a JSON record to path | tools.rs:908 |
| `accessibility_review_save` | destructive |   | doc,path,decisions,rule,decision,page,note,replace | Save accessibility review decisions — Write a local JSON sidecar for an open PDF with rule-level human review decisions | tools.rs:919 |
| `accessibility_review_load` | ro |   | doc,path | Open accessibility review decisions — Read a local review sidecar only when its recorded SHA-256 matches the open PDF | tools.rs:922 |
| `accessibility_review_html` | destructive |   | before,after,path,all | Accessibility review summary — Compare two open versions of a PDF and write a self-contained HTML summary of the same evidence as accessibility_review_record: document hashes, outco | tools.rs:925 |
| `accessibility_fix` |   |   | doc,rule,value | Fix an accessibility problem — Apply the checker's automatic fix for a rule: primary-language (value: the language, e.g | tools.rs:936 |
| `accessibility_figures` | ro | `a11y.alt_text` | doc | List figures — The tagged figures (Figure elements, through the role map) in document order: figure number, page, alternate text and where the figure is drawn (top-l | tools.rs:938 |
| `accessibility_set_alt` |   | `a11y.alt_text` | doc,figure,alt,decorative | Set alternate text — Set a figure's alternate text (alt; empty or omitted clears it), or mark it decorative (decorative: true: its content becomes an artifact and the figu | tools.rs:942 |
| `accessibility_tags` | ro | `a11y.tags` | doc | List structure tags — List reachable indirect structure tags in document order with PDF object number, generation, nearest indirect parent, depth, page, type, title, actual | tools.rs:945 |
| `accessibility_set_tag_property` |   |   | doc,object,generation,property,value | Edit tag property — Set or clear one property of a reachable indirect structure tag | tools.rs:949 |
| `accessibility_move_tag` |   |   | doc,object,generation,anchor,anchor_generation,placement | Move structure tag — Move a reachable indirect structure tag before or after a direct sibling in their parent's /K array, changing tagged reading order without changing pa | tools.rs:956 |
| `form_merge_data` |   | `form.merge_data` | paths,path | Merge data files into spreadsheet — Collect the field values of form data files (FDF, XFDF) or filled-in PDF forms into one CSV file at path: a column per field name, a row per file | tools.rs:964 |
| `js_run` |   | `tools.js_console` | doc,script,field | Run JavaScript — Run Acrobat JavaScript in the document, as the JavaScript console does (or as push button `field`'s Mouse Up script when field is given; on a laid-out | tools.rs:967 |
| `js_document_scripts` | ro | `tools.document_js` | doc | Document JavaScripts — List the document-level JavaScripts (name and source), which define functions field scripts use. | tools.rs:973 |
| `js_set_document_script` |   | `tools.document_js` | doc,name,script | Edit a document JavaScript — Add or replace the document-level JavaScript `name` with `script`, or delete it (script omitted) | tools.rs:977 |
| `form_set_script` |   | `form.prepare` | doc,field,event,script | Set a field's JavaScript — Field Properties ▸ Run custom script: set field's JavaScript for event keystroke, format, validate or calculate (the field's actions; calculated field | tools.rs:980 |
| `doc_export_office` |   | `export.docx` | doc,path | Export to Word, PowerPoint, HTML or RTF — Export a PDF ▸ Word (.docx), PowerPoint (.pptx, one slide per page), HTML (.html, one file with images inline) or RTF (.rtf), chosen by path's extensi | tools.rs:986 |
| `pdfa_verify` | ro | `standards.pdfa` | doc,level | Verify PDF/A — Standards ▸ Verify PDF/A compliance: the PDF/A-2b or 3b rules the document breaks (ISO 19005 clause, message, page, whether Save as PDF/A can fix it), | tools.rs:989 |
| `print_readiness` | ro |   | doc | Print readiness — Verification only | tools.rs:993 |
| `pdfa_convert` |   | `standards.pdfa` | doc,level | Save as PDF/A — Standards ▸ Save as PDF/A: fix what can be fixed for PDF/A-2b or 3b (XMP identification and metadata, an sRGB output intent, forbidden actions and Jav | tools.rs:996 |
| `action_list` | ro | `actions.wizard` |   | List actions — Action Wizard: the built-in actions (name, description, steps) and every step an action can use (id, label, whether it takes a text argument). | tools.rs:999 |
| `action_run` |   | `actions.wizard` | action,steps,step,arg,paths,folder | Run an action — Action Wizard: run a built-in action (action: its name) or a list of steps ([{step, arg}], ids from action_list) on each PDF in paths, writing the res | tools.rs:1003 |
| `doc_compare` | ro | `doc.compare` | doc,other,limit,visual | Compare files — Compare the text of two open documents: other is the older version, doc the newer | tools.rs:1014 |
| `doc_compare_report` |   | `doc.compare` | doc,other,path | Compare report — Write the compare summary report (a PDF listing every change) to path. | tools.rs:1018 |
| `doc_compare_mark` |   | `doc.compare` | doc,other | Mark differences as comments — Add the differences from other (older) to doc (newer) as comments in doc: highlights over replaced (blue) and inserted (green) text, notes where text | tools.rs:1021 |
| `form_detect_fields` |   | `form.detect` | doc,pages,add | Detect form fields — Prepare a form ▸ automatic field detection: find the blanks a printed form asks to be filled (underscore runs, lines, empty boxes, small squares for c | tools.rs:1024 |
| `form_actions` | ro | `form.prepare` | doc,field | Field actions — Field Properties ▸ Actions: the field's action for each trigger (mouse_up, mouse_down, mouse_enter, mouse_exit, on_focus, on_blur). | tools.rs:1027 |
| `form_set_actions` |   | `form.prepare` | doc,field,actions,trigger,javascript,url,reset,menu,page,show,hide,submit | Set field actions — Field Properties ▸ Actions: replace the field's actions | tools.rs:1031 |
| `js_enabled` |   |   | enabled | JavaScript on or off — Preferences ▸ JavaScript ▸ Enable Acrobat JavaScript: set it with `enabled`, or read it | tools.rs:1041 |
| `ocr_recognize` |   | `ocr.recognize` | doc,pages,dpi,language,skip_text_pages | Recognize text (OCR) — Scan & OCR ▸ Recognize text: render pages, read the words in them and add them as invisible text over the page image, so scanned pages become searchab | tools.rs:1043 |
| `ocr_recognize_files` |   | `ocr.recognize_batch` | paths,folder,dpi,language,skip_text_pages | Recognize text in multiple files — Scan & OCR ▸ Recognize text ▸ In multiple files: read every page of each PDF in paths and write the searchable result into folder under the same name | tools.rs:1055 |
| `ocr_status` | ro | `ocr.recognize` |   | OCR status — Whether text recognition is available (its models are installed: release packages include them; a source build fetches them with `cargo xtask models`; | tools.rs:1067 |
| `doc_export_all_images` |   | `export.all_images` | doc,folder,pages,min_size | Export all images — Write the images that pages use into a folder (`<name>_Page_<n>_Image_<k>.jpg\|png`), each once: JPEG images unchanged, others as PNG with their soft m | tools.rs:1071 |
| `doc_export_text` |   | `export.text` | doc,path,pages | Export text — Write the reading-order text of pages to a .txt file (pages separated by form feeds). | tools.rs:1082 |
| `fill_sign_add` |   | `sign.fill.text` | doc,page,at,text,font_size,path,scan,sensitivity,language,author | Fill & Sign: type text or place a mark — Fill in a form that has no fields, as Acrobat's Fill & Sign does: type text (`text`, default 10 pt; `font_size` 4–300 pt), place a check, cross, dot o | tools.rs:1085 |
| `fill_sign_date_format` |   |   | language | Date format for Fill & Sign — Preferences ▸ Date format: the pattern Fill & Sign dates use (default m/d/yyyy) | tools.rs:1108 |
| `doc_create` |   |   | from,paths,dpi,text,path,pages,width,height,name | Create a PDF — Create a new, unsaved document and return it like doc_open: `blank` (pages, width, height in points; default 1 US Letter page), `images` (paths of PNG | tools.rs:1110 |
| `doc_reduce` |   | `optimize.reduce` | doc,path | Reduce file size — Write a smaller copy of the document to `path` with Acrobat's Reduce File Size choices: images above 225 ppi downsampled to 150 ppi and JPEG-compresse | tools.rs:1129 |
| `doc_initial_view` |   |   | doc,navigation,layout,magnification,page,fit_window,center_window,full_screen,display_title,hide_menubar,hide_toolbar,hide_window_ui | Initial view — Read or change how the document opens (Document Properties ▸ Initial View) and its reading options: navigation (page, bookmarks, pages, attachments, l | tools.rs:1132 |
| `doc_audit_space` | ro | `optimize.advanced` | doc | Audit space usage — How many bytes each kind of content takes and its share of the file (PDF Optimizer ▸ Audit space usage): images, content streams, fonts, forms, commen | tools.rs:1151 |
| `text_lines` | ro | `edit.edit_text` | doc,page | List text lines — The lines of existing text on a page (Edit a PDF ▸ Edit text): number, text, box (top-left-origin points), font and size | tools.rs:1155 |
| `image_hit` | ro | `edit.edit_text` | doc,page,point | Hit test page artwork — List image/Form numbers whose placement and all clipping paths contain point [x,y] in top-left-origin points | tools.rs:1159 |
| `page_images` | ro | `edit.edit_text` | doc,page | List page images — The raster images and grouped Form artwork a page draws (Edit a PDF): number, full placement rect and visible_rect after page/path clipping (top-left- | tools.rs:1163 |
| `image_edit` |   | `edit.edit_text` | doc,page,image,action,rect,quarters,path | Edit an image — Change one of a page's images (number from page_images): action move (rect: new full placement box in top-left-origin points, based on page_images.rec | tools.rs:1167 |
| `image_save` | destructive | `edit.edit_text` | doc,page,image,path | Save image as — Write one of a page's raster images (number from page_images; forms are refused) to path: JPEG images unchanged, others as PNG (the extension is added | tools.rs:1181 |
| `text_font_files` | ro | `edit.edit_text` | directories | List installed document fonts — Discover openly licensed, embeddable TrueType/OpenType files on request | tools.rs:1185 |
| `text_font_import` |   | `edit.edit_text` | doc,page,path | Load document font — Load an explicitly selected, openly licensed full TrueType/OpenType face into a page | tools.rs:1188 |
| `text_fonts` | ro | `edit.edit_text` | doc,page | List document fonts — List validated embedded source faces and approved full preview faces for a page | tools.rs:1191 |
| `text_paragraphs` | ro | `edit.edit_text` | doc,page | List paragraphs — The paragraphs on a page (adjacent source runs grouped into aligned visual rows, including mixed fonts, sizes and colours): number, text, editor_text | tools.rs:1194 |
| `text_edit_layout` | ro | `edit.edit_text` | doc,page,paragraph,text,size,width,font_resource,ranges | Measure text draft — Measure a paragraph draft using the same PDF rewrite as Apply, without changing the document | tools.rs:1198 |
| `text_edit` |   | `edit.edit_text` | doc,page,line,paragraph,font_resource,ranges,range,font,bold,italic,size,color | Edit text — Replace the text of one paragraph (paragraph, from text_paragraphs: rewrapped to the paragraph's width with its line spacing) or one line (line, from | tools.rs:1202 |
| `doc_revisions` |   |   | doc | List revisions — List the document's saved revisions (oldest first): each incremental update is one | tools.rs:1225 |
| `doc_open_revision` |   |   | doc,revision | Open a revision — Open saved revision `revision` (1 = the oldest) of a document as a new, unsaved document, to see the file as it was then. | tools.rs:1227 |
| `doc_optimize` |   | `optimize.advanced` | doc,path,color,gray,discard_thumbnails,discard_alternate_images,discard_tags,discard_print_settings,flate_unencoded,remove_invalid_links,remove_unreferenced_dests,discard | Optimize PDF — Write an optimized copy to `path` (Acrobat's PDF Optimizer) | tools.rs:1229 |
| `doc_flatten` | destructive |   | doc,comments,fields | Flatten — Merge comment and/or form field appearances into the page content, so they print and display everywhere but can no longer be edited | tools.rs:1248 |
| `edit_undo` |   | `edit.undo` | doc | Undo — Undo the last edit of a document. | tools.rs:1251 |
| `edit_redo` |   | `edit.redo` | doc | Redo — Redo the last undone edit of a document. | tools.rs:1252 |
| `command_list` | ro |   | doc,filter,enabled_only | List commands — Every registered PdfCraft command with its menu, shortcut, whether it is enabled now, and the tool that automates it. | tools.rs:1253 |
| `command_run` | destructive |   | id,params | Run a command — Run a registry command through its headless tool | tools.rs:1256 |
| `command_batch` | destructive |   | steps,id,params | Run several commands — Run steps in order (at most 1000) | tools.rs:1258 |
| `doc_inspect` | ro |   | doc | Inspect open documents — With doc, return doc_info; otherwise return information for every open document (empty documents array when none are open). | tools.rs:1260 |
| `render_preview` | ro |   | doc,page,max_side | Render a preview — Render a PNG bounded by max_side (default 1024), without changing or saving the document | tools.rs:1262 |

The tools most relevant to an asset browser are `doc_open`, `doc_info`/`doc_inspect` (metadata, page sizes, fonts), `render_preview` (bounded PNG, default max side 1024), `page_render` (dpi ≤ 600), `text_extract`/`text_find`, `doc_combine`, `page_extract`, `doc_split`, `doc_create` (`from: blank|images|text`), `doc_export_images`, `doc_export_all_images`, `doc_export_text`, `doc_export_office`, `doc_set_info`, `doc_batch`, `doc_reduce`/`doc_optimize`, `command_batch`.

### 4. MCP server

| Aspect | Detail | src |
| --- | --- | --- |
| Start | `pdfcraft-cli mcp [--root DIR] [--compact]`; never starts on its own | apps/pdfcraft-cli/src/main.rs:11-12, 798-812 |
| Transport | Newline-delimited JSON-RPC 2.0 over stdin/stdout; no network port | crates/automation/src/mcp.rs:1-12 |
| Protocol versions | `2025-06-18`, `2025-03-26`, `2024-11-05` | mcp.rs:21 |
| Auth | None (stdio, the caller owns the process); `--root` confines files | mcp.rs:3-6 |
| Methods | `initialize, ping, tools/list, tools/call, resources/list, resources/templates/list, resources/read, notifications/*` | mcp.rs:8, 128-188 |
| Tools | Exactly the 163 automation tools above (`tools/list`). `--compact` lists only 15 core tools (`doc_open, doc_info, doc_save, doc_close, page_render, text_extract, text_find, doc_combine, doc_split, edit_undo, command_list, command_run, command_batch, doc_inspect, render_preview`) plus the meta tools `tool_search`/`tool_call`; `tools/call` accepts any tool either way | mcp.rs:41-57, 72-79, 189-240 |
| Mirrors the registry? | Partly: tools carry `command` links, and `command_list`/`command_run`/`command_batch` expose registry ids, but only the 64 that have a tool run headless | tools.rs:1253-1258; conventions.rs:11-13 |
| Resources | Open documents, read-only: `pdfcraft://doc/{doc}/info` (JSON), `/text`, `/page/{page}/text`, `/page/{page}/image?dpi=1-600` (PNG) | mcp.rs:9-11 |
| Prompts | None | mcp.rs:8 (not in the implemented list) |
| Errors | Tool failures come back as `isError: true` results; protocol errors use JSON-RPC codes | mcp.rs:11-12, 23-26 |

### 5. Control channel (running GUI)

| Aspect | Detail | src |
| --- | --- | --- |
| Enable | `pdfcraft --control <file>` (off by default) | apps/pdfcraft/src/main.rs:16-19 |
| Port | `TcpListener::bind(("127.0.0.1", 0))`, so the OS picks a random port; it is published in the control file | crates/ui-egui/src/control.rs:789-793; main.rs:459-460 |
| Protocol | Newline-delimited **JSON-RPC 2.0** | control.rs:784-788 |
| Auth | The first request must be `{"method":"auth","params":{"token":…}}` (constant-time compare); anything else closes the connection. Lines are capped at 4 KiB before auth | control.rs:786-788, 831-839, 863 |
| Timeout | 30 s per request, then error `-32002`; frames wait up to 5 s while the window isn't drawn | control.rs:841-846, 55 |
| Errors | `-32700` parse, `-32600` too long, `-32001` unauthenticated, `-32000` method error | control.rs:819-846 |
| Client | `pdfcraft-cli ui --control FILE <method> k=v…` (adds the `ui.` prefix itself). It refuses a control file that is a symlink or not the user's own | apps/pdfcraft-cli/src/main.rs:814-868, 870-912 |

Methods (control.rs:14-40, 279-350): `ui.state`, `ui.commands`, `ui.command {id}` (no params forwarded), `ui.dock`, `ui.dock.reset`, `ui.set {key,value}` (the view options above, plus `select`, `default-mode`, `theme`), `ui.open {path}` (one path, opens as a tab, replies `{"opened"}` or `{"accepted","pending":true}`; control.rs:756-766), `ui.inspect {query?,role?,focused?,limit?}` (AccessKit tree), `ui.click {id|label|x,y,button?}`, `ui.focus`, `ui.resize {width,height}`, `ui.quit`, `ui.move`, `ui.drag`, `ui.type {text}`, `ui.key {key,modifiers?}`, `ui.screenshot {region?}` (`png_base64`).

```json
{"jsonrpc":"2.0","id":1,"method":"auth","params":{"token":"<from control file>"}}
{"jsonrpc":"2.0","id":1,"result":{"ok":true}}
{"jsonrpc":"2.0","id":2,"method":"ui.open","params":{"path":"/abs/a.pdf"}}
{"jsonrpc":"2.0","id":2,"result":{"opened":"/abs/a.pdf","password_prompt":false}}
```

It targets the running GUI, but only one started with `--control`.

### 6. Library crates (no UI deps)

Every crate is MIT OR Apache-2.0 and versioned `0.8.0` in the workspace (Cargo.toml:20-50). The only UI crate is `pdfcraft-ui-egui`. `pdfcraft-automation` is also free of egui and safe to embed.

| Crate | Layer | Purpose | Notable deps |
| --- | --- | --- | --- |
| `pdfcraft-geom` | L0 | points, rects, page transforms | none |
| `pdfcraft-filters` | L0 | Flate/LZW/predictors/ASCII85/RunLength | flate2 |
| `pdfcraft-crypt` | L0 | standard security handler R2–R6 | aes, md-5, sha2 |
| `pdfcraft-cos` | L1 | lazy, tolerant object parser; incremental and full writer | crypt, filters |
| `pdfcraft-model` | L2 | typed page tree with inherited attributes; **`Page::display_size`** | cos |
| `pdfcraft-content` | L2 | content-stream operators and serializer | cos |
| `pdfcraft-fonts` | L2 | metrics/encoding for generated appearances | skrifa 0.44, harfrust 0.12 |
| **`pdfcraft-render`** | L3 | rasterization (vendored hayro) and inspection | cos, fonts, hayro, hayro-syntax, lopdf, kurbo, bytemuck |
| `pdfcraft-annot`, `-forms`, `-js`, `-xfa` | L3 | annotations, AcroForm, JS (boa), XFA | |
| `pdfcraft-organize` | L4 | rotate/delete/move/insert/**import/extract pages** (`import_pages`, `extract_pages`) | content, cos |
| `pdfcraft-create` | L4 | blank PDFs and PDFs **from images** (PNG, JPEG, TIFF, GIF, BMP) and text (`from_images`, crates/create/src/lib.rs:566-571) | png, tiff, image, zune-jpeg |
| `pdfcraft-export` | L4 | PDF to HTML/DOCX/RTF | flate2 |
| `pdfcraft-edit`, `-redact`, `-sign`, `-ocr`, `-optimize`, `-preflight`, `-a11y`, `-compare`, `-measure`, `-print`, `-xfdf`, `-audit` | L4 | one crate per Acrobat feature | (`print` pulls `craft-print` from git) |
| `pdfcraft-engine` | L6 | `Session` façade: `combine`, `combine_ranges`, `create_from_images`, `extract`, … (crates/engine/src/lib.rs:3865-4000) | every L4 crate |
| `pdfcraft-automation` | L7 | headless tool table and MCP server | engine, render, organize, platform |

**AssetCraft pin check (0.7.1 at 7163ba3 → 0.8.0):**

- **The patch set is unchanged.** At both tags `[patch.crates-io]` lists `hayro-interpret`, `hayro`, `hayro-syntax`, `hayro-jbig2` and `lopdf` (plus the UI-only `winit` and `egui-wgpu`, which AssetCraft doesn't need) (Cargo.toml:105-112; `git show 7163ba3:Cargo.toml` is identical). Vendored versions: hayro 0.7.1, hayro-interpret 0.7.0, hayro-syntax 0.7.2, hayro-jbig2 0.3.0, lopdf 0.45.0 (vendor/*/Cargo.toml).
- **The vendored source changed** between the tags: `vendor/hayro-interpret/src/{color.rs, font/true_type.rs, interpret/text.rs}`, `vendor/hayro-syntax/src/filter/lzw_flate.rs` and `vendor/hayro/src/renderer.rs` (`git diff --stat 7163ba3 v0.8.0 -- vendor`). Re-point the five patch entries at the v0.8.0 tree, together with `pdfcraft-render`.
- **The `pdfcraft-render` API is source-compatible.** `PageRenderer::new(Arc<Vec<u8>>, RenderConfig)`, `page_count()`, `render(RenderRequest) -> RenderedPage`, `RenderConfig {password, layers, hide_comments, reject_oversize, reading_colors}` and `RenderRequest {page, kind, tile, scale, tag, preview}` are unchanged (crates/render/src/raster.rs:55-68, 154-167, 197-211, 412-437; the raster.rs diff only adds tests). The one public change is `FontInfo` gaining `substitute: Option<String>` and `substitution: Option<String>` (crates/render/src/inspect.rs:368-374). That breaks only struct literals or exhaustive destructuring of `FontInfo`.
- **Cheap page-size query.** `pdfcraft-render` still has no dedicated call. `inspect()` returns `DocInfo.pages[i].{width,height,crop,rotation,label}` (inspect.rs:93-104, 405), but it walks the outline, annotations, fields and fonts, which can take seconds on very large files (inspect.rs:376-380). The cheap path that exists now is `pdfcraft_cos::Document::open(bytes)` (crates/cos/src/document.rs:255) plus `pdfcraft_model::pages(&doc)` and `Page::display_size(&doc)` (crates/model/src/lib.rs:52-57, 84). These are lazy, need no hayro, and honour `/Rotate` and `/UserUnit`. `PageRenderer::page_count()` (raster.rs:427) and `check_pages()` (inspect.rs:381) give the count only.
- **Text for search:** `RenderRequest { kind: RequestKind::Text, .. }` gives `RenderedPage.text: Arc<PageText>`, and `PageText::plain_text()` returns the string (raster.rs:136-143, 205; crates/render/src/text.rs:88).
- **Thumbnails:** set `preview: true` for low-detail renders (raster.rs:162-166). `RenderPool` (raster.rs:706-827) is a threaded queue.
- **DesignCraft interplay:** DesignCraft pins hayro from `storytold/pdfcraft@b5467d93` by git (designcraft Cargo.toml:105-108). That commit isn't in this shallow clone, so it couldn't be compared. AssetCraft's own `[patch.crates-io]` wins for the whole graph, so pointing it at PdfCraft v0.8.0's vendor also patches `designcraft-render`'s hayro. Note that PdfCraft's lockfile has `vello_cpu 0.0.8` (through hayro) while WordCraft and DesignCraft use `vello_cpu 0.2` (wordcraft Cargo.toml:53, designcraft Cargo.toml:53). The two versions coexist in one build; it costs binary size, not a conflict.

### 7. File formats

| Direction | Formats | src |
| --- | --- | --- |
| Open | PDF (`application/pdf` only in the .desktop file); the Open dialog also offers images and text, which it converts | packaging/linux/ai.storyteller.pdfcraft.desktop:14; crates/ui-egui/src/files.rs:64-73 |
| Create | blank; images (PNG, JPEG, TIFF all pages, GIF, BMP; JP2/J2K/JPX in the UI picker); `.txt` (WinAnsi only); clipboard | tools.rs:1110-1128; crates/ui-egui/src/create_ui.rs:153 |
| Save/export | PDF (incremental or full), PNG/JPEG/TIFF page images, text, DOCX, PPTX (UI), HTML, RTF, all embedded images, PDF/A, XFDF/FDF/XML/CSV form data | commands.rs `export.*`; tools `doc_export_*` |
| Templates | None (no template concept) | — |

### 8. Opportunities for AssetCraft (PdfCraft)

| AssetCraft wants | Surface that supports it today | Opportunity / suggested fix | Owner |
| --- | --- | --- | --- |
| Open selected file(s) in the running instance | Windows: launch `pdfcraft.exe a.pdf b.pdf` with no options and it hands off over the UDS (main.rs:216-224). macOS: `open -a PdfCraft …` (apple_events.rs:3-5). Any OS: control `ui.open {path}`, but only if the GUI was started with `--control FILE` | Extend `single_instance` to Unix-domain sockets on Linux and macOS so files-only launches hand off everywhere. Let `ui.open` take `paths[]` | PdfCraft |
| First-page thumbnail, headless | Library: `PageRenderer::render(RenderRequest{page:0,scale,preview:true,..})`. CLI: `render f.pdf --page 1 --dpi 36 --out t.png`. Tool: `render_preview {max_side}` | Works today. Consider a `render_thumbnail(bytes, max_side)` helper that sizes the scale from the page box | PdfCraft |
| Cheap page size / count | `pdfcraft-cos` + `pdfcraft-model::pages()[i].display_size()`; `PageRenderer::page_count()` | Add `pdfcraft_render::page_sizes(bytes, password) -> Vec<[f32;2]>` so AssetCraft doesn't need two more crates | PdfCraft |
| Text and metadata for search | `inspect()` → `DocInfo {title, author, subject, keywords, creator, producer, pdf_version, pages, fonts, …}`; `RequestKind::Text`; CLI `info`/`text`; tools `doc_info`, `text_extract` | Works today. XMP isn't surfaced in `DocInfo`; `doc_set_info` writes XMP, so a read-side `xmp` field would round-trip | PdfCraft |
| Merge or assemble PDFs from a selection | CLI `combine a b … --out`; tool `doc_combine {paths,pages,groups,out}`; library `Session::combine` / `combine_ranges` (engine lib.rs:3977-3995) or `pdfcraft_organize::import_pages` | Works headless. In the GUI, `ui.command page.combine` can't take params (control.rs:288); forward `params` to the linked tool | PdfCraft |
| Images → PDF (contact-sheet-like) | `pdfcraft --create-images imgs…`; tool `doc_create {from:"images",paths,dpi}`; `pdfcraft_create::from_images` | One page per image only. N-up layout belongs to DesignCraft, or to `doc_print`'s `per_sheet` (print imposition) | — |
| Export | Tools `doc_export_images`, `doc_export_all_images`, `doc_export_text`, `doc_export_office`; CLI `render` | Works today | — |
| Batch over a selection | CLI `batch` / tool `doc_batch` (convert, ocr, optimize); `run --script` | `run --script` takes a JSON array only; add JSONL to match DesignCraft's `script` | PdfCraft |

---

## WordCraft (v0.7.0, fc41918)

Workspace `version = "0.7.0"`, MIT OR Apache-2.0 (Cargo.toml:7-9). Layers are in docs/architecture.md:15-31 (L0 `geom`; L1 `doc`/`fonts`/`proof`; L2 `layout`/`docx`/`docbin`/`formats`; L3 `render`/`pdf`; L4 `engine`/`zotero`/`control-key`; L5 `mcp`; L6 `ui-egui`). docs/architecture.md:27 says the engine has "422 `CommandSpec`s"; the source has 617.

### 1. Desktop app launch (`apps/wordcraft/src/main.rs`)

Usage: `wordcraft [--control <port>] [--sample] [files…]` (main.rs:3).

| Arg / env | Effect | src |
| --- | --- | --- |
| `files…` | Each runs `file.open {path}` in the one window (one `Session`) | main.rs:252, 319-321, 346-350 |
| `--control <port>` / `WORDCRAFT_CONTROL_PORT` | Starts the loopback JSON-lines control server on that **fixed** port | main.rs:5-8, 240, 246, 334-339 |
| `--sample` | Opens the sample document | main.rs:247, 317 |
| `--version` | `wordcraft <ver> (<sha>)` | main.rs:248-251 |
| `WORDCRAFT_NO_PREFS` | No prefs, logs or user fonts (agent runs) | main.rs:148-151 |
| `WORDCRAFT_WAYLAND` | Keeps native Wayland (default is XWayland, for drag and drop) | main.rs:294-303 |
| `WORDCRAFT_FILE_DIALOG=builtin` | Uses the app's own file browser | src/file_dialogs.rs:93 |
| `WORDCRAFT_REGION` | Overrides the region (paper and units) | src/region.rs:13-19 |
| `RUST_LOG`, `WGPU_DX12_COMPILER` | | src/logging.rs:284; src/graphics.rs:98 |

**Single instance: none.** Every launch is a new process and window. With no files and no `--control`, it starts on File › Home (main.rs:343-345). On macOS, Finder opens arrive as Apple events (main.rs:304-309).

### 2. CLI (`apps/wordcraft-cli`)

Usage is at apps/wordcraft-cli/src/main.rs:13-46, dispatch at main.rs:184-402, and exit codes at main.rs:439-447 (0 on success or a closed stdout, 1 with `wordcraft-cli: <msg>` on stderr).

| Subcommand | Args | Output | src |
| --- | --- | --- | --- |
| `convert <in> <out> [--markup] [--format wordml2003\|flatopc]` | Any open format → docx, pdf, odt, rtf, html, md, tex, txt, json, png, xml | File; `wrote X` on stderr | main.rs:189-215 |
| `info <file>` | | Pretty JSON from `file.info`: `path, dirty, pages, words, paragraphs, properties{title,subject,creator,keywords,description,category,lastModifiedBy,created,modified,revision}, trackChanges, comments, sections, encrypted, layoutMs` | main.rs:216-221; crates/engine/src/cmd/file.rs:351-365 |
| `text <file>` | | Plain text (list labels kept) | main.rs:222-227 |
| `inspect <file>` | | `document.inspect` JSON | main.rs:228-233 |
| `render <file> <out.png> [--page N] [--scale S=2]` | | `file.exportPng` result JSON | main.rs:234-242 |
| `run [--file F \| --template T] --cmd 'id={json}' [--cmd …] [--save OUT] [--print] [--author NAME] [--page N --scale S]` | Commands run in argument order; `--print` echoes `id: result` | `--save` by extension (`.png` → exportPng) | main.rs:243-291 |
| `commands [--json]` | | Table, or `[{id,label,location,shortcut,params,mutates}]` | main.rs:292-306; crates/engine/src/lib.rs:121-129 |
| `parity [--markdown]` | | Feature-catalog parity | main.rs:307-316 |
| `mcp [--connect HOST:PORT \| --author NAME] [--automation-read-root DIR] [--automation-write-root DIR]` | Headless, or bridged to the app | stdio MCP | main.rs:317-359 |
| `zotero <cmd> <file> [--cmd …] [--save OUT] [--trace] [--port P]` | Zotero word-processor protocol | | main.rs:360-398 |

There is **no script-file or JSONL mode**: `run` takes repeated `--cmd` arguments only.

### 3. Command registry

`CommandSpec {id, label, location, shortcut, params: &str (agent-readable), mutates, enabled, run}` is at crates/engine/src/lib.rs:59-73, built with `CommandSpec::new(id,label,location,run).key().params().pure().when()` (lib.rs:79-101). The `Registry` with `get`, `all`, `describe` and `by_shortcut` is at lib.rs:102-135. Specs come from one `specs()` per module in `crates/engine/src/cmd/*.rs`, collected in `cmd::registry()` (crates/engine/src/cmd/mod.rs:1-80+). Total: **617** engine commands. The app adds UI-layer `ui.*` commands handled in `WordApp::ui_command` (crates/ui-egui/src/lib.rs:995-1205): `ui.tab, ui.backstage, ui.dialog, ui.closeDialog, ui.changePicture, ui.contextMenu, ui.collapseRibbon, ui.keytips, ui.dark, ui.theme, ui.interfaceSize, ui.animations, ui.language, ui.saveChanges, ui.openFileDialog, ui.fileBrowser, ui.openRecipientList, ui.printSettings, ui.print, ui.printSubmit, ui.makeDefaultApp, ui.discord, ui.caretAnimation, ui.startScreen, ui.linkTextBox`, plus `ui.zotero.*` (crates/ui-egui/src/zotero.rs:1-2).

Namespaces: arrange 14, canvas 4, caret 16, chart 5, design 11, developer 16, document 6, draw 16, edit 29, equation 20, fields 8, file 27, font 3, format 39, hf 3, insert 40, layout 18, list 2, mailings 17, object 1, outline 26, para 50, picture 14, readAloud 7, references 25, review 35, select 14, shape 9, smartArt 8, styles 16, table 48, text 21, tools 11, view 34, wordArt 4.

#### 3a. All command ids (compact, by namespace)

- **arrange** (14): `arrange.rotate`, `arrange.rotation`, `arrange.wrap`, `arrange.wrapText`, `arrange.moveWithText`, `arrange.position`, `arrange.bringForward`, `arrange.sendBackward`, `arrange.align`, `arrange.selectionPane`, `arrange.bounds`, `arrange.nudge`, `arrange.group`, `arrange.ungroup`
- **canvas** (4): `canvas.fit`, `canvas.expand`, `canvas.scale`, `canvas.member`
- **caret** (16): `caret.left`, `caret.right`, `caret.up`, `caret.down`, `caret.wordLeft`, `caret.wordRight`, `caret.home`, `caret.end`, `caret.paraUp`, `caret.paraDown`, `caret.docStart`, `caret.docEnd`, `caret.pageUp`, `caret.pageDown`, `caret.set`, `caret.pastCitation`
- **chart** (5): `chart.type`, `chart.editData`, `chart.title`, `chart.legend`, `chart.dataLabels`
- **design** (11): `design.theme`, `design.themeFonts`, `design.themeColors`, `design.styleSet`, `design.paragraphSpacing`, `design.watermark`, `design.pageColor`, `design.pageBorders`, `design.setDefault`, `design.themes`, `design.effects`
- **developer** (16): `developer.richText`, `developer.plainText`, `developer.picture`, `developer.buildingBlock`, `developer.checkBox`, `developer.comboBox`, `developer.dropDown`, `developer.datePicker`, `developer.repeatingSection`, `developer.designMode`, `developer.properties`, `developer.control`, `developer.toggleCheckBox`, `developer.chooseItem`, `developer.setDate`, `developer.removeControl`
- **document** (6): `document.inspect`, `document.text`, `document.paragraph`, `document.selection`, `document.layout`, `document.setText`
- **draw** (16): `draw.select`, `draw.lasso`, `draw.lassoMove`, `draw.lassoDelete`, `draw.lassoColor`, `draw.eraser`, `draw.pen`, `draw.pencil`, `draw.highlighter`, `draw.addPen`, `draw.removePen`, `draw.stroke`, `draw.erase`, `draw.inkToShape`, `draw.convertInk`, `draw.replay`
- **edit** (29): `edit.undo`, `edit.undoTo`, `edit.undoHistory`, `edit.redo`, `edit.cut`, `edit.copy`, `edit.paste`, `edit.pasteText`, `edit.pasteMerge`, `edit.clipboardPane`, `edit.clipboardItems`, `edit.pasteClipboardItem`, `edit.pasteAllClipboard`, `edit.deleteClipboardItem`, `edit.clearClipboard`, `edit.find`, `edit.advancedFind`, `edit.findNext`, `edit.findPrevious`, `edit.replace`, `edit.findReplace`, `edit.replaceAll`, `edit.goto`, `edit.goBack`, `edit.formatPainter`, `edit.copyFormat`, `edit.pasteFormat`, `edit.pasteSpecial`, `edit.repeat`
- **equation** (20): `equation.edit`, `equation.exit`, `equation.type`, `equation.insertStructure`, `equation.insertSymbol`, `equation.move`, `equation.backspace`, `equation.delete`, `equation.enter`, `equation.convert`, `equation.inputFormat`, `equation.normalText`, `equation.display`, `equation.justify`, `equation.number`, `equation.set`, `equation.get`, `equation.gallery`, `equation.structureActions`, `equation.structure`
- **fields** (8): `fields.toggleCode`, `fields.setCode`, `fields.unlink`, `fields.lock`, `fields.unlock`, `fields.next`, `fields.previous`, `fields.selected`
- **file** (27): `file.new`, `file.open`, `file.save`, `file.saveAs`, `file.encrypt`, `file.exportPdf`, `file.exportPng`, `file.printMarkup`, `file.printPdf`, `file.print`, `file.close`, `file.properties`, `file.info`, `file.setAuthor`, `file.privacyOptions`, `file.privacy`, `file.options`, `file.accessibility`, `file.inspect`, `file.protect`, `file.versions`, `file.recover`, `file.newFromTemplate`, `file.saveTemplate`, `file.autosave`, `file.compatibility`, `file.share`
- **font** (3): `font.add`, `font.list`, `font.remove`
- **format** (39): `format.bold`, `format.italic`, `format.underline`, `format.doubleUnderline`, `format.wordUnderline`, `format.strikethrough`, `format.border`, `format.doubleStrikethrough`, `format.subscript`, `format.superscript`, `format.allCaps`, `format.smallCaps`, `format.hidden`, `format.outline`, `format.shadow`, `format.emboss`, `format.engrave`, `format.font`, `format.size`, `format.growFont`, `format.shrinkFont`, `format.growFont1`, `format.shrinkFont1`, `format.color`, `format.highlight`, `format.shading`, `format.changeCase`, `format.clear`, `format.resetChar`, `format.spacing`, `format.scale`, `format.position`, `format.charStyle`, `format.set`, `format.fontDialog`, `format.enclose`, `format.state`, `format.phonetic`, `format.textEffects`
- **hf** (3): `hf.next`, `hf.previous`, `hf.position`
- **insert** (40): `insert.canvas`, `insert.chart`, `insert.crossReference`, `insert.equation`, `insert.pageBreak`, `insert.blankPage`, `insert.coverPage`, `insert.table`, `insert.picture`, `insert.icon`, `insert.shape`, `insert.textBox`, `insert.link`, `insert.removeLink`, `insert.bookmark`, `insert.header`, `insert.footer`, `insert.pageNumber`, `insert.editHeader`, `insert.editFooter`, `insert.closeHeader`, `insert.removeHeader`, `insert.removeFooter`, `insert.dateTime`, `insert.symbol`, `insert.field`, `insert.dateField`, `insert.timeField`, `insert.pageField`, `insert.dropCap`, `insert.horizontalLine`, `insert.textFromFile`, `insert.smartArt`, `insert.quickParts`, `insert.autoText`, `insert.docProperty`, `insert.signatureLine`, `insert.object`, `insert.spreadsheet`, `insert.wordArt`
- **layout** (18): `layout.margins`, `layout.orientation`, `layout.size`, `layout.columns`, `layout.break`, `layout.lineNumbers`, `layout.hyphenation`, `layout.manualHyphenation`, `layout.pageSetup`, `layout.verticalAlign`, `layout.sectionRtl`, `layout.textDirection`, `layout.differentFirstPage`, `layout.differentOddEven`, `layout.pageNumberFormat`, `layout.documentGrid`, `layout.section`, `layout.linkToPrevious`
- **list** (2): `list.define`, `list.get`
- **mailings** (17): `mailings.start`, `mailings.recipients`, `mailings.editRecipients`, `mailings.insertField`, `mailings.addressBlock`, `mailings.greetingLine`, `mailings.rules`, `mailings.matchFields`, `mailings.highlightFields`, `mailings.preview`, `mailings.next`, `mailings.previous`, `mailings.findRecipient`, `mailings.checkErrors`, `mailings.finish`, `mailings.envelopes`, `mailings.labels`
- **object** (1): `object.altText`
- **outline** (26): `outline.promoteToHeading1`, `outline.promote`, `outline.demote`, `outline.demoteToBody`, `outline.level`, `outline.moveUp`, `outline.moveDown`, `outline.expand`, `outline.collapse`, `outline.toggle`, `outline.selectSubtree`, `outline.showLevel`, `outline.showLevel1`, `outline.showLevel2`, `outline.showLevel3`, `outline.showLevel4`, `outline.showLevel5`, `outline.showLevel6`, `outline.showLevel7`, `outline.showLevel8`, `outline.showLevel9`, `outline.showAll`, `outline.firstLineOnly`, `outline.showFormatting`, `outline.close`, `outline.state`
- **para** (50): `para.alignLeft`, `para.alignCenter`, `para.alignRight`, `para.justify`, `para.distribute`, `para.align`, `para.rtl`, `para.ltr`, `para.indent`, `para.outdent`, `para.hangingIndent`, `para.removeHanging`, `para.indents`, `para.lineSpacing`, `para.single`, `para.double`, `para.oneAndHalf`, `para.spacing`, `para.addSpaceBefore`, `para.removeSpaceAfter`, `para.reset`, `para.style`, `para.normal`, `para.heading1`, `para.heading2`, `para.heading3`, `para.bullets`, `para.numbering`, `para.multilevel`, `para.listLevel`, `para.restartNumbering`, `para.moveUp`, `para.moveDown`, `para.promote`, `para.demote`, `para.shading`, `para.borders`, `para.sort`, `para.keepNext`, `para.keepLines`, `para.pageBreakBefore`, `para.widowControl`, `para.asianTypography`, `para.tabs`, `para.outlineLevel`, `para.set`, `para.dialog`, `para.setNumberingValue`, `para.defineBullet`, `para.defineNumber`
- **picture** (14): `picture.size`, `picture.crop`, `picture.altText`, `picture.corrections`, `picture.color`, `picture.iconColor`, `picture.effects`, `picture.transparency`, `picture.removeBackground`, `picture.compress`, `picture.reset`, `picture.change`, `picture.style`, `picture.border`
- **readAloud** (7): `readAloud.playPause`, `readAloud.next`, `readAloud.previous`, `readAloud.stop`, `readAloud.speed`, `readAloud.skipCitations`, `readAloud.status`
- **references** (25): `references.sources`, `references.citation`, `references.citationStyle`, `references.bibliography`, `references.tableOfFigures`, `references.updateFigures`, `references.markCitation`, `references.tableOfAuthorities`, `references.updateAuthorities`, `references.noteOptions`, `references.researcher`, `references.markEntry`, `references.index`, `references.updateIndex`, `references.toc`, `references.updateToc`, `references.removeToc`, `references.addText`, `references.footnote`, `references.endnote`, `references.convertNotes`, `references.nextFootnote`, `references.notes`, `references.caption`, `references.updateFields`
- **review** (35): `review.hideInk`, `review.restrict`, `review.newComment`, `review.reply`, `review.editComment`, `review.deleteComment`, `review.resolveComment`, `review.nextComment`, `review.previousComment`, `review.comments`, `review.trackChanges`, `review.acceptAll`, `review.rejectAll`, `review.accept`, `review.reject`, `review.nextChange`, `review.previousChange`, `review.markup`, `review.trackingOptions`, `review.wordCount`, `review.changes`, `review.spelling`, `review.issues`, `review.suggestions`, `review.addToDictionary`, `review.ignoreAll`, `review.applySuggestion`, `review.proofing`, `review.thesaurus`, `review.readAloud`, `review.compare`, `review.combine`, `review.language`, `review.showMarkup`, `review.editor`
- **select** (14): `select.all`, `select.range`, `select.word`, `select.sentence`, `select.paragraph`, `select.dragUnit`, `select.line`, `select.text`, `select.collapse`, `select.column`, `select.addObject`, `select.objects`, `select.similar`, `select.extend`
- **shape** (9): `shape.connect`, `shape.fill`, `shape.outline`, `shape.effects`, `shape.change`, `shape.textDirection`, `shape.alignText`, `shape.link`, `shape.breakLink`
- **smartArt** (8): `smartArt.items`, `smartArt.textPane`, `smartArt.addShape`, `smartArt.promote`, `smartArt.demote`, `smartArt.layout`, `smartArt.colors`, `smartArt.reset`
- **styles** (16): `styles.inspector`, `styles.inspect`, `styles.inspectorClear`, `styles.create`, `styles.modify`, `styles.updateToMatch`, `styles.delete`, `styles.manage`, `styles.setVisibility`, `styles.list`, `styles.pane`, `styles.apply`, `styles.addToGallery`, `styles.organizer`, `styles.copyFrom`, `styles.rename`
- **table** (48): `table.insertRowAbove`, `table.insertRowBelow`, `table.insertColumnLeft`, `table.insertColumnRight`, `table.deleteRow`, `table.deleteColumn`, `table.deleteTable`, `table.deleteCells`, `table.merge`, `table.split`, `table.splitTable`, `table.style`, `table.look`, `table.shading`, `table.borders`, `table.cellAlign`, `table.textDirection`, `table.autofit`, `table.distributeColumns`, `table.distributeRows`, `table.columnWidth`, `table.rowHeight`, `table.repeatHeader`, `table.altText`, `table.sort`, `table.toText`, `table.formula`, `table.selectTable`, `table.selectRow`, `table.selectColumn`, `table.selectCell`, `table.rtl`, `table.properties`, `table.fromText`, `table.quick`, `table.draw`, `table.eraser`, `table.selectAt`, `table.insertAt`, `table.moveDrag`, `table.fitColumn`, `table.resizeDrag`, `table.newStyle`, `table.modifyStyle`, `table.deleteStyle`, `table.cellMargins`, `table.borderPainter`, `table.viewGridlines`
- **text** (21): `text.overtype`, `text.insert`, `text.newParagraph`, `text.lineBreak`, `text.pageBreak`, `text.columnBreak`, `text.tab`, `text.backTab`, `text.backspace`, `text.delete`, `text.deleteWordBack`, `text.deleteWordForward`, `text.nbsp`, `text.nbHyphen`, `text.optionalHyphen`, `text.toggleUnicode`, `text.emDash`, `text.copyright`, `text.registered`, `text.trademark`, `text.ellipsis`
- **tools** (11): `tools.templates`, `tools.attachTemplate`, `tools.detachTemplate`, `tools.linkStyles`, `tools.updateStyles`, `tools.recordMacro`, `tools.macros`, `tools.autocorrect`, `tools.customizeKeyboard`, `tools.customizeRibbon`, `tools.search`
- **view** (34): `view.fieldCodes`, `view.immersive`, `view.vertical`, `view.sideToSide`, `view.marks`, `view.printLayout`, `view.webLayout`, `view.draft`, `view.outline`, `view.readMode`, `view.readColumnWidth`, `view.readTextSize`, `view.readLineSpacing`, `view.hideWhiteSpace`, `view.focus`, `view.ruler`, `view.gridlines`, `view.navigationPane`, `view.zoom`, `view.zoom100`, `view.zoomIn`, `view.zoomOut`, `view.onePage`, `view.multiplePages`, `view.pageWidth`, `view.darkMode`, `view.stylesPane`, `view.commentsPane`, `view.accessibilityPane`, `view.altTextPane`, `view.newWindow`, `view.split`, `view.state`, `view.numeral`
- **wordArt** (4): `wordArt.style`, `wordArt.textFill`, `wordArt.textOutline`, `wordArt.transform`

#### 3b. Commands relevant to an asset browser (detailed)

| id | label | ribbon location | mutates | params | src |
| --- | --- | --- | --- | --- | --- |
| `file.new` | New | File | no | {"template"?: "blank\|sample\|letter\|resume\|report"} | cmd/file.rs:36 |
| `file.open` | Open | File | no | {"path": string, "data"?: base64 (the file's bytes; `path` then only names it), "password"?: string (for a password-protected Word document)} | cmd/file.rs:37 |
| `file.save` | Save | File | no | {"path"?: string, "password"?: string (encrypt this and later saves with it, as file.encrypt does), "markup"?: bool (Print Markup for a PDF, PNG, HTML, Markdown or text copy: comments shown; default file.printMarkup), "format"?: "wordml2003" (an .xml file as a Word 2003 XML Document) \| "flatopc" (as a Word XML Document, the default; saving over a Word 2003 XML file keeps that format)} | cmd/file.rs:41 |
| `file.saveAs` | Save As | File | no | {"path": string, "password"?: string, "markup"?: bool, "format"?: string (as file.save)} | cmd/file.rs:45 |
| `file.encrypt` | Encrypt with Password | File › Info › Protect Document | no | {"password": string (Word documents are saved encrypted with it) \| null or "" (saved unencrypted)} | cmd/file.rs:48 |
| `file.exportPdf` | Export PDF | File › Export | no | {"path": string, "markup"?: bool (Print Markup: tracked changes marked up and each page scaled down beside the comments in balloons; default file.printMarkup)} | cmd/file.rs:51 |
| `file.exportPng` | Export Page as PNG | File › Export | no | {"path": string, "page"?: n (1-based), "scale"?: px per pt, "markup"?: bool (the page with its comments in balloons beside it; default file.printMarkup)} | cmd/file.rs:60 |
| `file.printMarkup` | Print Markup | File › Print | no | {"value"?: bool (omit to toggle)} — PDFs, prints and copies show comments (balloons beside each page; a list at the end of HTML, Markdown and text) and tracked changes | cmd/file.rs:64 |
| `file.printPdf` | Print to PDF | File › Print | no | {"path": string, "range"?: "all" \| "current" \| "selection" (the pages the selection touches) \| "custom", "pages"?: "1-3,5" (implies custom), "parity"?: "all" \| "odd" \| "even", "copies"?: 1-999, "collate"?: bool (default true)} | cmd/file.rs:70 |
| `file.print` | Print | File | no | {} | cmd/file.rs:75 |
| `file.close` | Close | File | no | {} | cmd/file.rs:81 |
| `file.properties` | Properties | File › Info | yes | {"title"?, "subject"?, "author"?, "keywords"?, "comments"?, "category"?, "custom"?: {name: string \| null (removes)}} | cmd/file.rs:87 |
| `file.info` | Info | File | no | {} | cmd/file.rs:89 |
| `file.setAuthor` | User Name | File › Options › General | no | {"name": string (author of new tracked changes and comments)} | cmd/file.rs:90 |
| `file.privacyOptions` | Privacy Options | File › Options › Save | yes | {"removePersonalInfo"?: bool (save comment and revision authors as "Author", no creator or last-modified-by), "removeDateTime"?: bool (save comments and tracked changes without dates)} — at least one; the open document keeps its real names and dates | cmd/file.rs:97 |
| `file.privacy` | Privacy Settings | File › Options › Save | no | {} → {"removePersonalInfo": bool, "removeDateTime": bool} (file.privacyOptions changes them) | cmd/file.rs:112 |
| `document.inspect` | Inspect Document | Agents | no | {"text"?: bool} | cmd/file.rs:115 |
| `document.text` | Document Text | Agents | no | {} | cmd/file.rs:116 |
| `document.paragraph` | Paragraph Details | Agents | no | {"path": [n], "story"?: Story} | cmd/file.rs:121 |
| `document.selection` | Selection | Agents | no | {} | cmd/file.rs:122 |
| `document.layout` | Layout Summary | Agents | no | {} | cmd/file.rs:131 |
| `document.setText` | Replace Document Text | Agents | yes | {"text": string} | cmd/file.rs:132 |
| `insert.coverPage` | Cover Page | Insert › Pages | yes | {"title"?: string, "subtitle"?: string, "author"?: string} | cmd/insert.rs:25 |
| `insert.picture` | Pictures | Insert › Illustrations | yes | {"path"?: string, "data"?: base64, "width"?: pt, "alt"?: string} | cmd/insert.rs:27 |
| `insert.textFromFile` | Text from File | Insert › Text › Object | yes | {"path": string} | cmd/insert.rs:93 |
| `mailings.start` | Start Mail Merge | Mailings › Start Mail Merge | no | {"kind": "letters\|emails\|envelopes\|labels\|directory\|normal"} | cmd/mailings.rs:39 |
| `mailings.recipients` | Select Recipients | Mailings › Start Mail Merge | no | {"csv"?: string, "path"?: string (.csv/.tsv/.txt, or a .xlsx/.xlsm/.ods workbook), "data"?: base64 (a file's bytes, as for path), "sheet"?: name \| number (1 = first; a workbook with several sheets and no sheet answers {"sheets": [...], "chooseSheet": true} and loads nothing), "headers"?: bool (default true: the first non-blank row names the fields; false: fields are Column 1…N), "rows"?: [{field: value}] \| [[value]], "fields"?: [string] (column order; required with array rows)} | cmd/mailings.rs:49 |
| `mailings.editRecipients` | Edit Recipient List | Mailings › Start Mail Merge | no | {"rows"?: [{field: value}] \| [[value]], "fields"?: [string]} | cmd/mailings.rs:52 |
| `mailings.insertField` | Insert Merge Field | Mailings › Write & Insert Fields | yes | {"field": string} | cmd/mailings.rs:62 |
| `mailings.addressBlock` | Address Block | Mailings › Write & Insert Fields | yes | {} | cmd/mailings.rs:68 |
| `mailings.greetingLine` | Greeting Line | Mailings › Write & Insert Fields | yes | {} | cmd/mailings.rs:71 |
| `mailings.rules` | Rules | Mailings › Write & Insert Fields | yes | {"rule": "IF\|SKIPIF\|NEXTIF\|NEXT\|MERGEREC", "field"?, "value"?, "then"?, "else"?} | cmd/mailings.rs:74 |
| `mailings.matchFields` | Match Fields | Mailings › Write & Insert Fields | no | {} | cmd/mailings.rs:93 |
| `mailings.highlightFields` | Highlight Merge Fields | Mailings › Write & Insert Fields | no | {} | cmd/mailings.rs:97 |
| `mailings.preview` | Preview Results | Mailings › Preview Results | yes | {} | cmd/mailings.rs:102 |
| `mailings.next` | Next Record | Mailings › Preview Results | yes | {} | cmd/mailings.rs:109 |
| `mailings.previous` | Previous Record | Mailings › Preview Results | yes | {} | cmd/mailings.rs:110 |
| `mailings.findRecipient` | Find Recipient | Mailings › Preview Results | yes | {} | cmd/mailings.rs:111 |
| `mailings.checkErrors` | Check for Errors | Mailings › Finish | no | {} | cmd/mailings.rs:125 |
| `mailings.finish` | Finish & Merge | Mailings › Finish | yes | {"path"?: string (save the merged document), "from"?: n, "to"?: n} | cmd/mailings.rs:135 |
| `mailings.envelopes` | Envelopes | Mailings › Create | yes | {"delivery": string, "return"?: string, "size"?: "Envelope #10\|#9\|Monarch\|DL\|C4\|C5\|C6\|B5", "width"?: pt, "height"?: pt, "add"?: bool (add to this document instead of a new one)} | cmd/mailings.rs:139 |
| `mailings.labels` | Labels | Mailings › Create | yes | {"text"?: string, "product"?: "letter-3x10\|letter-2x10\|letter-2x7\|letter-2x5\|letter-4x20\|a4-3x7\|a4-2x7\|a4-3x8\|a4-2x4", "rows"?: n, "cols"?: n, "fromRecipients"?: bool, "single"?: {"row": n, "col": n}} | cmd/mailings.rs:142 |
| `picture.size` | Size | Picture Format › Size | yes | {"width"?: pt, "height"?: pt, "lockAspect"?: bool, "scale"?: percent} | cmd/objects.rs:85 |
| `picture.altText` | Alt Text | Picture Format › Accessibility | yes | {"text": string, "title"?: string, "decorative"?: bool} | cmd/objects.rs:97 |
| `picture.compress` | Compress Pictures | Picture Format › Adjust | yes | {"maxPixels"?: n} | cmd/objects.rs:195 |
| `picture.change` | Change Picture | Picture Format › Adjust | yes | {"path"?: string, "data"?: base64} | cmd/objects.rs:202 |
| `file.options` | Options | File | no | {"pane"?: "general\|display\|proofing\|save\|language\|accessibility\|advanced\|agents", "author"?: string, "initials"?: string, "units"?: "inches\|centimeters\|millimeters\|points\|picas\|auto" (auto: the region's), "region"?: string (a locale tag like pl-PL, saved: the paper File › New starts on, the default units and the decimal separator; auto: the system's), "typingReplacesSelection"?: bool, "insertKeyOvertype"?: bool, "overtype"?: bool, "marks"?: {"tabs"?: bool, "spaces"?: bool, "paragraphs"?: bool, "hidden"?: bool}, "checkSpelling"?: bool, "markGrammar"?: bool, "ignoreUppercase"?: bool, "ignoreNumbers"?: bool, "ignoreInternet"?: bool, "addWords"?: [string], "removeWords"?: [string]} — with any setting it changes them and opens nothing; without, it opens File › Options (at `pane`). Returns the settings. | cmd/options.rs:21 |
| `tools.templates` | Templates and Add-ins | View › Templates | no | {} → {"template": path\|null, "linkStyles": bool} (from the ribbon, the app shows the dialog) | cmd/templates.rs:31 |
| `tools.attachTemplate` | Attach Template | View › Templates › Templates and Add-ins | yes | {"path": string, "linkStyles"?: bool, "update"?: bool (update the styles from it now), "data"?: base64 (the template's bytes, for "update")} | cmd/templates.rs:34 |
| `tools.detachTemplate` | Detach Template | View › Templates › Templates and Add-ins | yes | {} | cmd/templates.rs:36 |
| `tools.updateStyles` | Update Styles from Template | View › Templates › Templates and Add-ins | yes | {"path"?: string (default: the attached template), "data"?: base64 (the file's bytes; `path` then only names it)} → {"added", "replaced"} | cmd/templates.rs:46 |
| `tools.macros` | Macros | View › Macros | no | {"run"?: name, "define"?: {"name": string, "steps": [{"command", "params"}]}, "delete"?: name} | cmd/tools.rs:363 |
| `review.compare` | Compare | Review › Compare | yes | {"path"?: string (revised document), "text"?: string (revised text), "original"?: string (original document; default: this one), "author"?: string (label changes with), "caseChanges"?: bool, "whiteSpace"?: bool, "formatting"?: bool (default true: differences count), "level"?: "word\|character", "showIn"?: "original\|revised\|new" (revised/new: an untitled document), "moves"\|"comments"\|"tables"\|"headersFooters"\|"footnotes"\|"textBoxes"\|"fields"?: bool (accepted; not honoured yet except tables, always compared)} | cmd/tools.rs:386 |
| `file.accessibility` | Check Accessibility | Review › Accessibility | no | {} | cmd/tools.rs:388 |
| `file.inspect` | Inspect Document | File › Info | yes | {"remove"?: ["comments", "revisions", "properties", "hidden", "headers"]} | cmd/tools.rs:389 |
| `file.protect` | Protect Document | File › Info | yes | {"mode": "none\|readOnly\|comments\|trackedChanges"} | cmd/tools.rs:391 |
| `file.versions` | Version History | File › Info | no | {"save"?: label, "restore"?: index} | cmd/tools.rs:396 |
| `file.recover` | Recover Unsaved Documents | File › Info | no | {} | cmd/tools.rs:414 |
| `file.newFromTemplate` | New from Template | File › New | no | {"path": string} | cmd/tools.rs:418 |
| `file.saveTemplate` | Save as Template | File › Save As | no | {"path": string} | cmd/tools.rs:428 |
| `file.autosave` | AutoSave | Quick Access Toolbar | no | {} | cmd/tools.rs:436 |
| `file.compatibility` | Check Compatibility | File › Info | no | {} | cmd/tools.rs:441 |
| `file.share` | Share | File | no | {} | cmd/tools.rs:456 |
| `insert.docProperty` | Document Property | Insert › Text › Quick Parts | yes | {"name": "Title\|Author\|Subject\|Keywords\|Comments\|Category"} | cmd/tools.rs:464 |
| `insert.object` | Object | Insert › Text | yes | {"path": string} | cmd/tools.rs:508 |

### 4. MCP server (`crates/mcp`)

| Aspect | Detail | src |
| --- | --- | --- |
| Start | `wordcraft-cli mcp` (headless `Session`) or `wordcraft-cli mcp --connect 127.0.0.1:PORT` (bridges to an app started with `--control PORT`) | docs/mcp.md:6-12; apps/wordcraft-cli/src/main.rs:317-359 |
| Transport | JSON-RPC 2.0 over stdio | crates/mcp/src/lib.rs:1-5 |
| Protocol | `2025-06-18` (also `2025-03-26`, `2024-11-05`) | crates/mcp/src/server.rs:11-12 |
| Auth | stdio, none. `--connect` reads the app key from `WORDCRAFT_CONTROL_KEY` or the key file and sends it to loopback addresses only | docs/control-protocol.md:42-47 |
| Confinement | `--automation-read-root` / `--automation-write-root` (headless only; an error with `--connect`) | docs/mcp.md:27-60; main.rs:322-327 |
| Tools (16) | `list_commands {query?}`, `execute {command, params}`, `batch {steps, keepGoing?}`, `new_document {template}`, `open_document {path}` (.docx .doc .odt .rtf .md .tex .html .txt .json, any text), `save_document {path}` (format from the extension: docx pdf odt rtf html md tex txt png), `type_text`, `select_text`, `get_text`, `inspect_document`, `render_page {page?, scale?}` (PNG image content), `parity`, plus app-only `screenshot`, `click`, `key`, `ui_inspect` | crates/mcp/src/tools.rs:57-147 (definitions), 168-270 (`call_tool`) |
| Mirrors the registry? | Yes: `execute` and `batch` run any of the 617 ids (and `ui.*` when connected); `list_commands` lists them | tools.rs:171-215; lib.rs:37-52 |
| Resources | Document (`document.inspect`) and commands (`engine.commands`), both JSON; no templates | server.rs:150-166 |
| Prompts | None | server.rs:135 (`capabilities: {tools, resources}`) |

### 5. Control channel (running GUI)

| Aspect | Detail | src |
| --- | --- | --- |
| Enable | `wordcraft --control 7981` or `WORDCRAFT_CONTROL_PORT=7981`; binds `127.0.0.1:<port>` (fixed, caller-chosen) | apps/wordcraft/src/control_server.rs:38-46; docs/control-protocol.md:3-5 |
| Protocol | JSON lines: `{"id","key","method","params"}` → `{"id","ok":true,"result"}` / `{"ok":false,"error"}` | docs/control-protocol.md:7-14 |
| Auth | 256-bit random hex key in `<settings>/control-key.local-<port>` (mode 0600, removed on exit). It goes as top-level `key` on every request, or once with `{"method":"auth","key":…}`. A bad key gets `unauthorized` and the connection closes | control_server.rs:77-104; docs/control-protocol.md:16-41; crates/control-key/src/lib.rs:9-17 |
| Limits | 1 MiB lines; 30 s to the first authorised request; bad JSON closes the connection | control_server.rs:18-22; docs/control-protocol.md:36-41 |
| Methods | `engine.execute {command, params}` (any command id incl. `ui.*`), `engine.commands {search?}`, `document.inspect`, `ui.inspect`, `ui.click`, `ui.clickText`, `ui.move`, `ui.drag`, `ui.press`, `ui.release`, `ui.wheel`, `ui.key`, `ui.text`, `ui.screenshot {path?}`, `ui.render {path?, page?, scale?}`, `ui.parity`, `ui.resize`, `ui.focus`, `app.quit`; **any other method name is treated as a command id** | crates/ui-egui/src/control.rs:111-300; docs/control-protocol.md:49-70 |
| Targets the running GUI? | Yes, one window per port. Run several windows on different ports | — |

```json
{"id":1,"key":"<hex>","method":"engine.execute","params":{"command":"file.open","params":{"path":"/abs/a.docx"}}}
{"id":1,"ok":true,"result":{"path":"/abs/a.docx","paragraphs":12,"words":340,"encrypted":false}}
```

### 6. Library crates (no UI deps)

All are MIT OR Apache-2.0, and all workspace deps are **path-only with no `version`**, so AssetCraft can consume them only as git deps pinned to `v0.7.0`.

| Crate | Layer | Purpose | Notable deps |
| --- | --- | --- | --- |
| `wordcraft-geom` | L0 | units (pt, twips, EMU, in, cm) | serde |
| `wordcraft-doc` | L1 | document model; `Document::plain_text(StoryRef)` (crates/doc/src/lib.rs:816); `Document.core: CoreProps {title, subject, creator, keywords, description, category, last_modified_by, created, modified, revision}` (doc/src/lib.rs:406-417, 588) | serde, unicode-segmentation |
| `wordcraft-fonts` | L1 | font DB (bundled OFL + system), shaping | skrifa 0.47, harfrust 0.13, wuff |
| `wordcraft-proof` | L1 | spelling and hyphenation | miniz_oxide |
| **`wordcraft-docx`** | L2 | DOCX/DOCM/DOTX/DOTM read/write: `read`, `read_with_password`, `read_xml`, `write` (crates/docx/src/lib.rs:23-64) | quick-xml, zip, aes, sha*, cfb |
| `wordcraft-docbin` | L2 | `.doc` reader | cfb, image |
| `wordcraft-formats` | L2 | TXT/MD/HTML/RTF/ODT/LaTeX | quick-xml, zip, image |
| **`wordcraft-layout`** | L2 | pagination: `layout(&doc, &mut LayoutCache, &LayoutOptions) -> DocLayout` (crates/layout/src/lib.rs:1717) | fonts, proof, kurbo |
| **`wordcraft-render`** | L3 | `render_page(&doc, &page, scale, &RenderOptions) -> Rendered` with `.to_png()/.to_jpeg(q)` (crates/render/src/lib.rs:38-72, 402-407) | vello_cpu 0.2, metafile, image |
| `wordcraft-pdf` | L3 | PDF export (krilla) | krilla 0.8 |
| `wordcraft-metafile` | L1 | WMF/EMF parsing | kurbo |
| **`wordcraft-engine`** | L4 | `Session`, registry, `io::open_bytes(name, bytes)` / `open_path` dispatch by extension (crates/engine/src/io.rs:9-31, 101-130) | every crate above + regex, image |
| `wordcraft-mcp`, `wordcraft-control-key`, `wordcraft-zotero` | L4–L5 | MCP server, key-file location, Zotero | |

**DOCX first-page thumbnail and text extraction.** The simplest route is `wordcraft-engine`: `io::open_bytes("x.docx", &bytes)` → `Session::new(doc).export_layout()` (crates/engine/src/session.rs:775-784) → `wordcraft_render::render_page(&s.doc, &layout.pages[0], scale, &Default::default()).to_png()`. This is exactly what the control `ui.render` does (crates/ui-egui/src/control.rs:280-288). A lighter graph is `wordcraft-docx::read` + `wordcraft-layout::layout(…, &LayoutOptions{view: Print, ..})` + `wordcraft-render`, which skips krilla, docbin, formats and regex. Text comes from `doc.plain_text(StoryRef::Body)` and metadata from `doc.core`. **Caveat:** layout always paginates the whole document; there is no "first page only" option (`LayoutOptions`, crates/layout/src/lib.rs:139-155).

### 7. File formats

| Direction | Formats | src |
| --- | --- | --- |
| Open | docx, docm, dotx, dotm, doc, dot, txt, md/markdown, html/htm, rtf, odt, tex/latex/ltx, xml (Flat OPC / Word 2003 XML), `wcraft.json`/json; unknown extensions are sniffed, with text as a fallback | crates/engine/src/io.rs:9-29, 194-210 |
| Save | docx, docm, dotx, dotm, pdf, txt, md, html, rtf, odt, tex, png, json, xml | io.rs:31 |
| Templates | `file.new {template: blank\|sample\|letter\|resume\|report}`, `file.newFromTemplate {path}` (.dotx), `file.saveTemplate`, `tools.attachTemplate`/`updateStyles` | cmd/file.rs:36; cmd/tools.rs:418-428; cmd/templates.rs:31-46 |
| Assets in | `insert.picture` (path or base64), `picture.change`, `insert.textFromFile`, `insert.object`, `mailings.recipients` (csv, tsv, txt, xlsx, xlsm, ods) | docs/mcp.md:41-46 |

### 8. Opportunities for AssetCraft (WordCraft)

| AssetCraft wants | Surface that supports it today | Opportunity / suggested fix | Owner |
| --- | --- | --- | --- |
| Open file(s) in the running instance | Control `engine.execute file.open {path}`, only when the app runs with `--control PORT` and AssetCraft reads the key file | (1) The control path calls `WordApp::execute`, which skips the unsaved-changes prompt that `WordApp::run` applies (crates/ui-egui/src/lib.rs:636-648 vs 682-699), so a dirty document is replaced silently. Route control `file.open` through `run`, or add `app.open` that prompts. (2) One document per window, with no hand-off: add single-instance hand-off (PdfCraft's `single_instance` pattern) and a multi-document or new-window open | WordCraft |
| First-page thumbnail, headless | CLI `render f.docx t.png --page 1 --scale 0.3`; library route in §6; MCP `render_page` | Add a page-limited layout (`LayoutOptions::max_pages`), or read `docProps/thumbnail.*` when present: no reader exists (no `thumbnail` match in crates/docx/src) | WordCraft |
| Text and metadata for search | CLI `text`, `info`; library `plain_text` + `CoreProps`; command `document.text` | Works today | — |
| Batch or scripted edits | `run --cmd …` repeated; MCP `batch` | Add `run --script FILE.jsonl` (DesignCraft's `script` format) for parity across the suite | WordCraft |
| Export | `convert`; `file.exportPdf`, `file.exportPng`, `file.save {path:*.odt…}` | Works today | — |
| Depend on crates | path-only versions | Add `version = "0.7.0"` to workspace deps (as PdfCraft does), and a lean feature for engine that leaves out pdf/proof/zotero | WordCraft |

---

## DesignCraft (v0.8.0, 11a51d5)

Workspace `version = "0.8.0"`, MIT OR Apache-2.0 (Cargo.toml:7-9). Layers are in docs/architecture.md:15-35 (L0 `geom`/`color`, L1 `doc`/`fonts`, L2 `compose`/`images`, L3 `render`/`pdf`/`idml`/`epub`/`textimport`/`format`, L4 `tools`, L5 `engine`, L6 `ui-egui`/`mcp`). The doc says 446 commands (docs/architecture.md:30); the source has 459.

### 1. Desktop app launch (`apps/designcraft/src/main.rs`)

Usage: `designcraft [--control <port>] [--sample] [files…]` (main.rs:3).

| Arg / env | Effect | src |
| --- | --- | --- |
| `files…` | Each runs `file.open {path}` and opens as a **new document** in the same process (multi-document) | main.rs:258, 346-350 |
| `--control <port>` / `DESIGNCRAFT_CONTROL_PORT` | JSON-lines control server on `127.0.0.1:<port>`; an invalid value is logged and ignored | main.rs:5-7, 231-240, 246, 252, 339-341 |
| `--sample` | `file.newSample` | main.rs:253, 343-345 |
| `--version` | `designcraft <ver>` | main.rs:254-257 |
| `DESIGNCRAFT_NO_PREFS`, `DESIGNCRAFT_CONFIG_DIR` | No prefs; isolated profile and recovery root | main.rs:94-97; src/profile.rs:25; docs/control-protocol.md:6-9 |
| `DESIGNCRAFT_WAYLAND`, `DESIGNCRAFT_NO_NATIVE_MENU`, `WGPU_BACKEND`, `RUST_LOG` | | main.rs:47, 297; main.rs:9-10; src/logging.rs:249 |

**Single instance: none.** Every launch is a new process. There is no Apple-events module (apps/designcraft/src has no `apple_events`). On start it reopens crash-recovered documents (`file.recovery.open`, main.rs:328).

### 2. CLI (`apps/designcraft-cli`)

Usage is at apps/designcraft-cli/src/main.rs:3-18 and 39, dispatch at main.rs:92-138. Exit code 0 on success; 1 with `designcraft-cli: <msg>` on stderr for errors (main.rs:140-148); a broken pipe exits 0 (main.rs:84-90).

| Subcommand | Args | Output | src |
| --- | --- | --- | --- |
| `run [--in FILE.designcraft\|.idml \| --sample] [--cmd ID[=JSON]]… [--page N] [--scale S] [--pdf-options JSON] [--export OUT.png\|.jpg\|.pdf\|.designcraft\|.idml\|.epub] [--all-pages DIR]` | Ordered. `--page`/`--scale`/`--pdf-options` apply to the exports after them. `--cmd` params can use `$N.path` | Each non-null result as a JSON line; writes on stderr | main.rs:220-318 |
| `commands [FILTER]` | | JSON array `[{id,label,menu,shortcut,params,enabled,disabled_reason?}]` | main.rs:100-112; crates/engine/src/cmd/mod.rs:84-108 |
| `describe ID` | | One command's JSON (suggests similar ids on a miss) | main.rs:321-344 |
| `script [FILE\|-] [--in FILE \| --sample] [--connect PORT] [--save OUT] [--export OUT]… [--keep-going]` | **Headless or against the running app.** One step per line (`command.id {json}`, `command.id={json}`, JSON lines, or a JSON array); `#`/`//` comments; `$N.path`, `${N.path}`, `$last` | `{"completed": n, "results": […]}` (+ `failedIndex/failedCommand/error`); non-zero exit on failure | main.rs:42-70, 347-437 |
| `app [--port PORT=7979] COMMAND [JSON]` / `app --method METHOD [JSON]` | One request to the running app | Pretty JSON | main.rs:439-466 |
| `mcp [--connect PORT\|HOST:PORT] [--sample]` | | stdio MCP | main.rs:162-194 |
| `perf …`, `bench FILE`, `validate-pdf FILE.pdf` (PDF/X-4; exit 1 if invalid), `links`, `--version` | | | main.rs:117-124, 150-158 |

### 3. Command registry

`CommandSpec {id, label, menu: &[&str], shortcut, params: &str, enabled, run, journal, undoable}` is at crates/engine/src/cmd/mod.rs:70-82. The `cmd!` macro has three forms: plain (undoable), `query` (no journal, no undo) and `noundo` (mod.rs:137-148). `command_specs()` collects every module's `specs()` (mod.rs:150-200+), and `CommandSpec::info` serializes `{id,label,menu,shortcut,params,enabled,disabled_reason}` (mod.rs:84-108). Total: **459** engine commands. The UI adds **106** commands from `menus::UI_COMMANDS` (crates/ui-egui/src/menus.rs:8+), which `engine.commands` returns with `"ui": true` (crates/ui-egui/src/control.rs:63-69).

Namespaces (engine): anchor 2, anchored 4, app 1, article 6, book 10, bookmark 4, button 3, changes 6, color 4, condition 5, conveyor 4, data 10, document 4, edit 14, endnote 5, file 29, find 5, font 2, footnote 6, form 3, frame 2, gap 1, guide 6, hyperlink 5, hyphenation 3, index 4, ink 2, layer 9, layout 19, library 8, line 1, links 7, liquid 2, list 2, math 2, media 2, note 5, object 56, page 2, path 12, place 3, preflight 1, prefs 1, script 1, select 8, selection 2, snippet 2, spelling 5, states 5, story 8, strokeStyle 3, style 21, swatch 10, table 35, text 16, toc 3, tool 3, transform 10, type 26, variables 4, xml 10, xref 5.

#### 3a. All engine command ids (compact)

- **anchor** (2): `anchor.create`, `anchor.list`
- **anchored** (4): `anchored.insert`, `anchored.options`, `anchored.release`, `anchored.list`
- **app** (1): `app.links`
- **article** (6): `article.list`, `article.new`, `article.add`, `article.remove`, `article.options`, `article.delete`
- **book** (10): `book.new`, `book.open`, `book.close`, `book.add`, `book.remove`, `book.styleSource`, `book.list`, `book.paginate`, `book.syncStyles`, `book.exportPdf`
- **bookmark** (4): `bookmark.rename`, `bookmark.add`, `bookmark.delete`, `bookmark.list`
- **button** (3): `button.set`, `button.clear`, `button.list`
- **changes** (6): `changes.track`, `changes.list`, `changes.accept`, `changes.reject`, `changes.acceptAll`, `changes.rejectAll`
- **color** (4): `color.settings`, `color.loadProfile`, `color.convert`, `color.sample`
- **condition** (5): `condition.list`, `condition.new`, `condition.options`, `condition.delete`, `condition.apply`
- **conveyor** (4): `conveyor.collect`, `conveyor.place`, `conveyor.list`, `conveyor.clear`
- **data** (10): `data.source.select`, `data.source.update`, `data.source.remove`, `data.fields`, `data.placeholder.add`, `data.placeholder.remove`, `data.options`, `data.preview`, `data.preview.stop`, `data.merge`
- **document** (4): `document.inspect`, `document.list`, `document.history`, `document.preferences`
- **edit** (14): `edit.undo`, `edit.redo`, `edit.selectAll`, `edit.deselectAll`, `edit.clear`, `edit.copy`, `edit.cut`, `edit.pasteWithoutFormatting`, `edit.paste`, `edit.pasteInto`, `edit.pasteInPlace`, `edit.duplicate`, `edit.stepAndRepeat`, `edit.transparencyBlendSpace`
- **endnote** (5): `endnote.insert`, `endnote.edit`, `endnote.delete`, `endnote.list`, `endnote.options`
- **file** (29): `file.exportPdf`, `file.exportEpub`, `file.exportFixedEpub`, `file.printBooklet`, `file.exportHtml`, `file.exportText`, `file.new`, `file.newSample`, `file.presets`, `file.open`, `file.openBytes`, `file.save`, `file.saveAs`, `file.saveACopy`, `file.revert`, `file.recovery.list`, `file.recovery.save`, `file.recovery.open`, `file.serialize`, `file.close`, `file.activate`, `file.place`, `file.exportIdml`, `file.openIdml`, `file.package`, `file.printers`, `file.print`, `file.exportXml`, `file.importXml`
- **find** (5): `find.objects`, `find.changeObjects`, `find.find`, `find.change`, `find.next`
- **font** (2): `font.list`, `font.replace`
- **footnote** (6): `footnote.insert`, `footnote.options`, `footnote.list`, `footnote.setText`, `footnote.delete`, `footnote.goToReference`
- **form** (3): `form.set`, `form.clear`, `form.list`
- **frame** (2): `frame.create`, `frame.grid`
- **gap** (1): `gap.move`
- **guide** (6): `guide.add`, `guide.move`, `guide.delete`, `guide.deleteAll`, `guide.list`, `guide.liquid`
- **hyperlink** (5): `hyperlink.create`, `hyperlink.delete`, `hyperlink.list`, `hyperlink.edit`, `hyperlink.goToSource`
- **hyphenation** (3): `hyphenation.addException`, `hyphenation.removeException`, `hyphenation.list`
- **index** (4): `index.addReference`, `index.list`, `index.generate`, `index.update`
- **ink** (2): `ink.list`, `ink.options`
- **layer** (9): `layer.move`, `layer.selectItems`, `layer.new`, `layer.set`, `layer.delete`, `layer.merge`, `layer.deleteUnused`, `layer.others`, `layer.activate`
- **layout** (19): `layout.createGuides`, `layout.pages.insert`, `layout.rotateSpreadView`, `layout.pages.delete`, `layout.pages.move`, `layout.pages.duplicateSpread`, `layout.pages.applyParent`, `layout.parents.new`, `layout.parents.edit`, `layout.marginsAndColumns`, `layout.documentSetup`, `layout.pages.toSpread`, `layout.spreadShuffle`, `layout.pageSize`, `layout.section`, `layout.createAlternate`, `layout.overrideParentItems`, `layout.removeOverrides`, `layout.detachAll`
- **library** (8): `library.new`, `library.open`, `library.list`, `library.json`, `library.add`, `library.place`, `library.remove`, `library.close`
- **line** (1): `line.create`
- **links** (7): `links.list`, `links.relink`, `links.relinkFolder`, `links.update`, `links.embed`, `links.goTo`, `links.copyTo`
- **liquid** (2): `liquid.pageRule`, `liquid.object`
- **list** (2): `list.define`, `list.delete`
- **math** (2): `math.insert`, `math.svg`
- **media** (2): `media.options`, `media.get`
- **note** (5): `note.new`, `note.edit`, `note.delete`, `note.convertToText`, `note.list`
- **object** (56): `object.caption`, `object.matchAttributes`, `object.arrange`, `object.bringToFront`, `object.bringForward`, `object.sendBackward`, `object.sendToBack`, `object.group`, `object.ungroup`, `object.lock`, `object.unlockAll`, `object.hide`, `object.showAll`, `object.setFlags`, `object.fill`, `object.swapFillStroke`, `object.defaultFillStroke`, `object.stroke`, `object.opacity`, `object.dropShadow`, `object.innerShadow`, `object.outerGlow`, `object.exportOptions`, `object.innerGlow`, `object.bevel`, `object.satin`, `object.directionalFeather`, `object.feather`, `object.content`, `object.textFrameOptions`, `object.cornerOptions`, `object.textWrap`, `object.fit`, `object.fittingOptions`, `object.rename`, `object.align`, `object.distribute`, `object.clippingPath`, `object.primaryTextFrame`, `object.attributes`, `object.globalLight`, `object.label`, `object.altText`, `object.transparencyGroup`, `object.findByLabel`, `object.setLayer`, `object.makeCompoundPath`, `object.releaseCompoundPath`, `object.pathfinder`, `object.gradientFeather`, `object.pdfLayers`, `object.layerOptions`, `object.qrCode`, `object.convertShape`, `object.gradient`, `object.color`
- **page** (2): `page.transition`, `page.transitions`
- **path** (12): `path.addAnchor`, `path.deleteAnchor`, `path.convertAnchor`, `path.erase`, `path.smooth`, `path.create`, `path.appendAnchor`, `path.setAnchor`, `path.reverse`, `path.close`, `path.moveAnchors`, `path.split`
- **place** (3): `place.styles`, `place.load`, `place.drop`
- **preflight** (1): `preflight.run`
- **prefs** (1): `prefs.set`
- **script** (1): `script.run`
- **select** (8): `select.firstAbove`, `select.nextAbove`, `select.nextBelow`, `select.lastBelow`, `select.container`, `select.content`, `select.nextInGroup`, `select.previousInGroup`
- **selection** (2): `selection.set`, `selection.toggle`
- **snippet** (2): `snippet.export`, `snippet.place`
- **spelling** (5): `spelling.check`, `spelling.setWords`, `spelling.words`, `spelling.addWord`, `spelling.change`
- **states** (5): `states.create`, `states.list`, `states.show`, `states.rename`, `states.release`
- **story** (8): `story.placeAndLink`, `story.links`, `story.updateLink`, `story.unlink`, `story.setDirection`, `story.setText`, `story.replaceRange`, `story.get`
- **strokeStyle** (3): `strokeStyle.list`, `strokeStyle.new`, `strokeStyle.delete`
- **style** (21): `style.compositeFont.list`, `style.compositeFont.set`, `style.exportTag`, `style.paragraph.apply`, `style.character.apply`, `style.paragraph.create`, `style.paragraph.edit`, `style.paragraph.delete`, `style.character.create`, `style.character.edit`, `style.list`, `style.object.apply`, `style.object.create`, `style.group`, `style.cell.create`, `style.cell.apply`, `style.cell.edit`, `style.table.create`, `style.table.apply`, `style.table.edit`, `style.breakLink`
- **swatch** (10): `swatch.create`, `swatch.load`, `swatch.save`, `swatch.addToSwatches`, `swatch.addUnnamed`, `swatch.delete`, `swatch.newColorGroup`, `swatch.moveToGroup`, `swatch.ungroupColorGroup`, `swatch.renameColorGroup`
- **table** (35): `table.insert`, `table.convertFromText`, `table.convertToText`, `table.insertRow`, `table.insertRowAbove`, `table.insertRowBelow`, `table.insertColumn`, `table.insertColumnLeft`, `table.insertColumnRight`, `table.deleteRow`, `table.deleteColumn`, `table.delete`, `table.merge`, `table.dropCells`, `table.placeGraphic`, `table.textCell`, `table.moveRow`, `table.moveColumn`, `table.sortRows`, `table.unmerge`, `table.splitHorizontally`, `table.splitVertically`, `table.setCell`, `table.setRowHeight`, `table.setColumnWidth`, `table.distributeColumns`, `table.distributeRows`, `table.options`, `table.select`, `table.selectTable`, `table.selectRow`, `table.selectColumn`, `table.nextCell`, `table.prevCell`, `table.get`
- **text** (16): `text.placeCaret`, `text.release`, `text.extendTo`, `text.selectWord`, `text.selectAt`, `text.selectSentence`, `text.selectParagraph`, `text.selectStory`, `text.select`, `text.insert`, `text.delete`, `text.move`, `text.exitToFrame`, `text.thread`, `text.unthread`, `text.removeFromThread`
- **toc** (3): `toc.generate`, `toc.update`, `toc.entries`
- **tool** (3): `tool.select`, `tool.list`, `tool.polygonSettings`
- **transform** (10): `transform.move`, `transform.resize`, `transform.rotate`, `transform.scale`, `transform.again`, `transform.clear`, `transform.shear`, `transform.flip`, `transform.set`, `transform.info`
- **type** (26): `type.onPath`, `type.pathOptions`, `type.createOutlines`, `type.fillWithPlaceholder`, `type.char`, `type.para`, `type.align`, `type.alignLeft`, `type.alignCenter`, `type.alignRight`, `type.justify`, `type.bold`, `type.italic`, `type.swapFillStroke`, `type.underline`, `type.allCaps`, `type.tateChuYoko`, `type.ruby`, `type.kenten`, `type.warichu`, `type.storyDirection`, `type.sizeUp`, `type.sizeDown`, `type.changeCase`, `type.openType`, `type.selectionAttrs`
- **variables** (4): `variables.list`, `variables.define`, `variables.delete`, `variables.insert`
- **xml** (10): `xml.tags`, `xml.newTag`, `xml.deleteTag`, `xml.tagText`, `xml.tag`, `xml.mapStyle`, `xml.structure`, `xml.loadDtd`, `xml.deleteDtd`, `xml.validate`
- **xref** (5): `xref.insert`, `xref.list`, `xref.formats`, `xref.defineFormat`, `xref.setFormat`

#### 3b. UI-layer command ids (106, app only)

- **app** (42): `app.newDocumentDialog`, `app.openDialog`, `app.placeDialog`, `app.saveDialog`, `app.save`, `app.quit`, `app.saveCopyDialog`, `app.loadSwatches`, `app.packageDialog`, `app.saveSwatches`, `app.exportPng`, `app.exportIdml`, `app.exportEpub`, `app.exportFixedEpub`, `app.exportHtml`, `app.exportXml`, `app.printBooklet`, `app.qrCode`, `app.userDictionary`, `app.fittingOptions`, `app.menus`, `app.graphicCell`, `app.placeAndLink`, `app.placeWithOptions`, `app.exportText`, `app.exportPdf`, `app.palette`, `app.preferences`, `app.keyboardShortcuts`, `app.findChange`, `app.insertTableDialog`, `app.footnoteOptionsDialog`, `app.rubyDialog`, `app.findFontDialog`, `app.insertXrefDialog`, `app.deleteAllGuides`, `app.deletePage`, `app.duplicateSpread`, `app.tablePanel`, `app.storyEditor`, `app.layerOptions`, `app.colorSettings`
- **color** (2): `color.toggleFillStroke`, `color.applyNone`
- **edit** (1): `edit.dynamicSpelling`
- **help** (6): `help.discord`, `help.appPage`, `help.github`, `help.issues`, `help.website`, `help.app`
- **view** (30): `view.zoomIn`, `view.zoomOut`, `view.fitPage`, `view.fitSelection`, `view.fitSpread`, `view.actualSize`, `view.entirePasteboard`, `view.zoom`, `view.screenMode`, `view.togglePreview`, `view.frameEdges`, `view.rulers`, `view.guides`, `view.snapToGuides`, `view.snapToDocumentGrid`, `view.smartGuides`, `view.baselineGrid`, `view.textThreads`, `view.hiddenCharacters`, `view.taggedFrames`, `view.tagMarkers`, `view.flattenerPreview`, `view.rotateSpread`, `view.proofColors`, `view.separations`, `view.overprintPreview`, `view.fastDisplay`, `view.typicalDisplay`, `view.highQualityDisplay`, `view.goToPage`
- **window** (25): `window.hideMenuItem`, `window.uiScale`, `window.richBlack`, `window.setShortcut`, `window.resetShortcuts`, `window.hidePanels`, `window.hidePanelsExceptTools`, `window.nextDocument`, `window.previousDocument`, `window.floatPanel`, `window.panel.float`, `window.panel.dock`, `window.panel.activate`, `window.panel.close`, `window.dockPanel`, `window.controlBar`, `window.split`, `window.newWindow`, `window.taskBar`, `window.taskBarReset`, `window.toolsDoubleColumn`, `window.newWorkspace`, `window.deleteWorkspace`, `window.resetWorkspace`, `window.brightness`

#### 3c. Commands relevant to an asset browser (detailed)

`kind`: `query` = read-only, no undo; `noundo` = changes state without an undo step; `undoable` = plain.

| id | label | menu | shortcut | kind | params | src |
| --- | --- | --- | --- | --- | --- | --- |
| `book.new` | New Book | File > New |   | noundo | {path} — an empty book file (.dcbook) | cmd/book.rs:241 |
| `book.open` | Open Book |   |   | noundo | {path} → {documents} | cmd/book.rs:248 |
| `book.close` | Close Book |   |   | noundo | {} | cmd/book.rs:260 |
| `book.add` | Add Document |   |   | noundo | {path, at?: index} | cmd/book.rs:264 |
| `book.remove` | Remove Document |   |   | noundo | {index} | cmd/book.rs:274 |
| `book.styleSource` | Style Source |   |   | noundo | {index} | cmd/book.rs:288 |
| `book.list` | Book |   |   | query | {} → {path, styleSource, documents: [{path, pages, firstPage}]} | cmd/book.rs:299 |
| `book.paginate` | Update Numbering |   |   | noundo | {} — each document starts numbering where the previous one ended (files are saved) | cmd/book.rs:310 |
| `book.syncStyles` | Synchronize Book |   |   | noundo | {} — paragraph and character styles and swatches of the style source go into every document (by name) | cmd/book.rs:313 |
| `book.exportPdf` | Export Book to PDF… |   |   | noundo | {path?} — every document in order as one PDF → {path, bytes, pages} (no path: {base64, …}) | cmd/book.rs:316 |
| `object.caption` | Generate Static Caption | Object > Captions |   | undoable | {text?: template (default "{name}"; variables {name} {path} {altText} {label} {ppi} {dimensions} {format}), position?: below\|above\|left\|right, offset? (pt, 0), height? (pt, 24), style?: paragraph style, live?: bool (kept current as the source changes), ids?} → {ids} | cmd/captions.rs:12 |
| `data.source.select` | Select Data Source… |   |   | undoable | {path \| bytes\|base64, name, delimiter?: comma\|tab\|semicolon, sheet?} → {fields, records, warnings} | cmd/datamerge/mod.rs:31 |
| `data.source.update` | Update Data Source |   |   | undoable | {} → re-read the linked file | cmd/datamerge/mod.rs:40 |
| `data.source.remove` | Remove Data Source |   |   | undoable | {} → drop the source; placeholders stay | cmd/datamerge/mod.rs:41 |
| `data.fields` | Data Fields |   |   | query | {csv? \| rows? \| path? \| bytes?, name?} → [{name, kind, uses}]; no params reads the linked source and does not attach one | cmd/datamerge/mod.rs:42 |
| `data.placeholder.add` | Insert Field |   |   | undoable | {field, role?: text\|image\|qr\|hyperlink, story?, at?, end?, item?} → {id} | cmd/datamerge/mod.rs:51 |
| `data.placeholder.remove` | Remove Field |   |   | undoable | {id} | cmd/datamerge/mod.rs:60 |
| `data.options` | Data Merge Options |   |   | undoable | {records?, one?, range?, perPage?, arrange?, insets?, columnSpacing?, rowSpacing?, fitting?, center?, linkImages?, limit?} | cmd/datamerge/mod.rs:61 |
| `data.preview` | Preview Record |   |   | query | {record?: n} → fill the template with that record (session only) | cmd/datamerge/mod.rs:70 |
| `data.preview.stop` | Stop Preview |   |   | noundo | {} → restore the unfilled template | cmd/datamerge/mod.rs:71 |
| `data.merge` | Create Merged Document… |   |   | undoable | {csv? \| rows? \| path? \| bytes?, name?, records?, one?, range?, perPage?, arrange?, insets?, columnSpacing?, rowSpacing?, fitting?, center?, linkImages?, limit?} → {records, pages, missingImages, oversetStories, warnings} | cmd/datamerge/mod.rs:72 |
| `file.exportPdf` | Export PDF… | File |   | noundo | {path?, pages?: "1-3,5" \| [1,3] (1-based positions; default all), flatten?: high\|medium\|low\|ppi (Transparency Flattener: spreads with transparency are rasterised), spreads?: bool, fullScreen?: bool, bookmarksPanel?: bool, pageLayout?: single\|continuous\|twoUp\|twoUpCover\|twoUpContinuous, view?: fitPage\|fitWidth\|actual, advanceSeconds?: number (interactive PDF), bleed?: bool (document bleed), marks?: bool \| {crop?, bleed?, pageInfo?, weight?, offset?}, standard?: "none"\|"x4"\|"a2b", compressImages?: bool, tagged?: bool (structure tree: stories as paragraphs, figures with alt text), media?: bool (interactive: embed placed video and sound), title?, author?} → {path, bytes, pages, warnings} (no path: {base64, …}) | cmd/export.rs:11 |
| `file.exportEpub` | Export EPUB (Reflowable)… | File |   | noundo | {path?, title?, author?, language?: "en", cover?: bool (the first page as the cover image), fixedLayout?: bool (pre-paginated: each page as an image with its text)} → {path, bytes} (no path: {base64, bytes}) | cmd/export.rs:14 |
| `file.exportFixedEpub` | Export EPUB (Fixed Layout)… | File |   | noundo | {path?, title?, author?, language?} — pre-paginated EPUB → {path, bytes} (no path: {base64, bytes}) | cmd/export.rs:17 |
| `file.printBooklet` | Print Booklet… | File |   | noundo | {path?, type?: saddleStitch\|twoUpConsecutive, spaceBetween? (pt)} — printer spreads as PDF (pages imposed in booklet order) → {path, bytes, sheets} (no path: {base64, …}) | cmd/export.rs:22 |
| `file.exportHtml` | Export HTML… | File |   | noundo | {path?, title?, language?} — one self-contained page (styles inline, images embedded), stories and graphics in reading order → {path, bytes} (no path: {text, bytes}) | cmd/export.rs:25 |
| `file.exportText` | Export Text… | File |   | noundo | {path?, format?: "txt"\|"rtf"\|"tagged" (Tagged Text; default from the path, else txt), story?, frame?} — the story being edited or of the selected frame → {path, bytes} (no path: {text, bytes}) | cmd/export.rs:28 |
| `file.new` | Document… | File > New | Cmd+N | noundo | {preset?: "Letter"\|"A4"\|…, width?, height?, pages?, facingPages?, columns?, gutter?, margins?: number\|{top,bottom,inside,outside}, bleed?, title?} | cmd/file.rs:15 |
| `file.newSample` | Sample Document | Help |   | noundo | {} — a multi-page magazine sample | cmd/file.rs:18 |
| `file.presets` | Document Presets |   |   | query | {} | cmd/file.rs:19 |
| `file.open` | Open… | File | Cmd+O | noundo | {path} — .designcraft or .idml; the fonts in a `Document Fonts` folder beside it load first → {index, documentFonts: faces loaded, warnings: font files skipped (and, for .idml, package parts missing)} | cmd/file.rs:20 |
| `file.openBytes` | Open Bytes |   |   | noundo | {name, base64} — DesignCraft JSON or an IDML package | cmd/file.rs:23 |
| `file.save` | Save | File | Cmd+S | noundo | {path?} | cmd/file.rs:24 |
| `file.saveAs` | Save As… | File | Cmd+Shift+S | noundo | {path} | cmd/file.rs:25 |
| `file.saveACopy` | Save a Copy… | File |   | noundo | {path} — writes the document without changing which file it is or its unsaved state | cmd/file.rs:26 |
| `file.revert` | Revert | File |   | noundo | {} — back to the last saved version (not undoable) | cmd/file.rs:35 |
| `file.recovery.list` | Recovery Data |   |   | query | {dir?} → [{uid, path, title, saved}] unsaved documents left by a crash (default: the session's recovery folder) | cmd/file.rs:36 |
| `file.recovery.save` | Save Recovery Data |   |   | noundo | {dir?} — write every unsaved document to the recovery folder (the app does this on a timer) | cmd/file.rs:40 |
| `file.recovery.open` | Recover Documents |   |   | noundo | {dir?} — reopen documents left in the recovery folder (unsaved) | cmd/file.rs:44 |
| `file.serialize` | Serialize |   |   | query | {} → {base64, bytes} the .designcraft file | cmd/file.rs:48 |
| `file.close` | Close | File | Cmd+W | noundo | {index?} | cmd/file.rs:55 |
| `file.activate` | Activate Document |   |   | noundo | {index} | cmd/file.rs:60 |
| `snippet.export` | Export Selection as Snippet | File > Export |   | query | {path?} — the selected items (with their stories, styles and images) as a .designcraft snippet; no path: {base64} | cmd/file.rs:64 |
| `place.styles` | Import Options: Styles |   |   | query | {path \| base64, name} → {paragraph: [name], character: [name], conflicts: [name]} — the styles a Word/RTF file brings (for styleMap / styleConflicts) | cmd/file.rs:66 |
| `snippet.place` | Place Snippet |   |   | undoable | {path \| base64, spread?, x?, y?} — items keep their positions unless x/y given (top-left) | cmd/file.rs:81 |
| `place.load` | Load Place Cursor |   |   | undoable | {path \| base64, name?} — load a graphic into the place cursor (then click/drag with the placeGun tool) | cmd/file.rs:90 |
| `place.drop` | Place Loaded Graphic |   |   | undoable | {spread?, x, y, rect?: [x0,y0,x1,y1], frame?: id} | cmd/file.rs:99 |
| `file.place` | Place… | File | Cmd+D | undoable | {path?\|base64?, name?, frame?: id (place into), spread?, x?, y?, width?, pdfPage?: n (1-based, Image Import Options), pdfCrop?: crop\|trim\|bleed\|art\|media, layoutPage?: n (IDML / .designcraft: that page's objects as a group)} — places an image (into the selected empty frame if any); text files (.txt, .docx, .rtf, .md) and Excel workbooks (.xlsx, as a table) go into the insertion point, the selected frame or a new frame on `page`/`rect` — {autoflow?: adds pages with threaded frames until the text fits, removeStyles?, styleMap?: {imported name: document style}, styleConflicts?: useExisting\|redefine\|autoRename} | cmd/file.rs:100 |
| `layout.createGuides` | Create Guides… | Layout |   | undoable | {rows?: 0..1000, columns?: 0..1000, rowGutter?: 12, columnGutter?: 12, fitTo?: margins\|page, removeExisting?: false, spread?, page? (index in the spread; default all pages)} — at most 10000 guides per command | cmd/guides.rs:53 |
| `document.inspect` | Inspect Document |   |   | query | {} → pages, spreads, items, stories (with overset), styles, swatches, selection | cmd/inspect.rs:11 |
| `document.list` | List Documents |   |   | query | {} | cmd/inspect.rs:14 |
| `document.history` | History |   |   | query | {} | cmd/inspect.rs:26 |
| `file.exportIdml` | Export IDML… |   |   | query | {path?, embedImages?: true} — writes an IDML package to `path`, or returns {base64} without a path | cmd/interchange.rs:12 |
| `file.openIdml` | Open IDML |   |   | noundo | {path \| base64, name?} — opens an IDML package as a new document (linked images are read next to the file or from its Links/ folder; the fonts in a `Document Fonts` folder beside it load first) → {index, documentFonts, warnings: package parts missing, font files skipped} | cmd/interchange.rs:15 |
| `layout.pages.insert` | Insert Pages… | Layout > Pages | Cmd+Shift+P | undoable | {count?: 1, after?: page index (default: last), parent?: "A"\|null} | cmd/layout.rs:80 |
| `layout.pages.delete` | Delete Pages | Layout > Pages |   | undoable | {pages: [index]} | cmd/layout.rs:117 |
| `layout.pages.move` | Move Pages… | Layout > Pages |   | undoable | {from, to} | cmd/layout.rs:130 |
| `layout.pages.duplicateSpread` | Duplicate Spread | Layout > Pages |   | undoable | {spread} | cmd/layout.rs:138 |
| `layout.pages.applyParent` | Apply Parent to Pages… | Layout > Pages |   | undoable | {pages: [index], parent: "A"\|null} | cmd/layout.rs:153 |
| `layout.marginsAndColumns` | Margins and Columns… | Layout |   | undoable | {pages?: [index] (default all), margins?: number\|{top,bottom,inside,outside}, columns?, gutter?, adjustLayout?: bool (objects follow the margins)} | cmd/layout.rs:189 |
| `layout.documentSetup` | Document Setup… | File | Cmd+Alt+P | undoable | {width?, height?, pages?: count, startPage?: n, facingPages?, binding?: leftToRight\|rightToLeft, intent?: print\|web\|mobile, bleed?, slug?: n \| [top, bottom, inside, outside], adjustLayout?: bool (objects follow the new page size)} → the document setup | cmd/layout.rs:230 |
| `layout.pages.toSpread` | Add Page to Spread | Layout > Pages |   | undoable | {page (1-based), spread (0-based)} — the page joins that spread (up to 10 pages; the spread stops shuffling) | cmd/layout.rs:337 |
| `layout.pageSize` | Page Size |   |   | undoable | {pages: [1-based page numbers], width?, height?, preset?: "A4"\|"Letter"\|…} — pages of their own size (Page tool); objects on the pages keep their place on them | cmd/layout.rs:374 |
| `library.new` | New Library | File > New |   | noundo | {path?} — an empty Object Library (saved to `path` when given) | cmd/library.rs:48 |
| `library.open` | Open Library |   |   | noundo | {path \| json} → {items} | cmd/library.rs:54 |
| `library.list` | Library Items |   |   | query | {} → [{index, name, description}] | cmd/library.rs:72 |
| `library.json` | Library Contents |   |   | query | {} → the library as JSON text (to save where there's no file system) | cmd/library.rs:76 |
| `library.add` | Add Item |   |   | noundo | {name?, description?} — the selection → {index} | cmd/library.rs:79 |
| `library.place` | Place Item |   |   | undoable | {index, spread?, x?, y?} — at its original position unless x/y (top-left) are given | cmd/library.rs:96 |
| `library.remove` | Delete Item |   |   | noundo | {index} | cmd/library.rs:111 |
| `conveyor.collect` | Collect |   |   | noundo | {ids? (default: the selection)} — each object onto the Content Collector conveyor → {count} | cmd/library.rs:122 |
| `conveyor.place` | Place |   |   | undoable | {index? (0), spread?, x?, y?, keep?: bool (stay on the conveyor)} — the Content Placer: the collected object at x/y (top-left) | cmd/library.rs:138 |
| `conveyor.list` | Conveyor |   |   | query | {} → [name] | cmd/library.rs:156 |
| `conveyor.clear` | Clear Conveyor |   |   | noundo | {} | cmd/library.rs:157 |
| `library.close` | Close Library |   |   | noundo | {} | cmd/library.rs:161 |
| `story.placeAndLink` | Place and Link | Edit |   | undoable | {story (parent), rect, spread?} — a new frame whose story copies the parent and stays linked → {id, story} | cmd/linked.rs:21 |
| `links.list` | Links |   |   | query | {} → [{asset, name, path, status: ok\|modified\|missing\|embedded, pixels, uses: [{id, page, ppi}]}] | cmd/links.rs:18 |
| `links.relink` | Relink… | Window > Links |   | undoable | {asset, path} — use another file for every placement of the graphic | cmd/links.rs:21 |
| `links.relinkFolder` | Relink to Folder… | Window > Links |   | undoable | {dir?: folder to look in (default: each link's own folder), extension?: e.g. "tif" (Relink File Extension), assets?: [ids], missingOnly?: bool (default true with `dir`)} → {relinked, notFound: [names]} | cmd/links.rs:30 |
| `links.update` | Update Link | Window > Links |   | undoable | {asset?} — reload modified links from their files (all when no asset) | cmd/links.rs:39 |
| `links.embed` | Embed Link | Window > Links |   | undoable | {asset} — keep only the copy in the document | cmd/links.rs:48 |
| `links.goTo` | Go To Link | Window > Links |   | noundo | {asset} — selects the frames showing the graphic | cmd/links.rs:56 |
| `frame.create` | Create Frame |   |   | undoable | {spread?: index \| {kind: doc\|parent, index} (a parent spread: index into `parents` of document.inspect), rect: [x0,y0,x1,y1] (spread coords), shape?: rectangle\|ellipse\|polygon, content?: graphic\|text\|unassigned, sides?: 6, text?: string, caret?: bool, vertical?: bool (text: a new vertical story)} | cmd/object.rs:30 |
| `object.exportOptions` | Object Export Options… | Object |   | undoable | {ids?, artifact?: bool (tagged PDF decoration), rasterize?: bool (EPUB/HTML image), align?: left\|center\|right\|"", pageBreakBefore?: bool, altText?} | cmd/object.rs:372 |
| `object.fit` | Fitting | Object > Fitting |   | undoable | {mode: fillProportionally\|fitProportionally\|fitContentToFrame\|centerContent\|fitFrameToContent, ids?} | cmd/object.rs:584 |
| `object.fittingOptions` | Frame Fitting Options… | Object > Fitting |   | undoable | {autoFit?: bool, fitting?: none\|fillProportionally\|fitProportionally\|fitContentToFrame\|centerContent, align?: 0..8 (reference point, 4 = centre), crop?: n \| [t, l, b, r] (negative adds space), ids?} | cmd/object.rs:593 |
| `edit.stepAndRepeat` | Step and Repeat… | Edit | Cmd+Alt+U | undoable | {count?: 1..1000, dx?, dy?, rows?, columns?, ids?} — copies of the selection (or ids) offset by (dx, dy); with rows/columns, a grid of at most 1000 cells, and at most 1000 item copies including group contents → {created, ids: the copies} | cmd/object.rs:606 |
| `frame.grid` | Gridify |   |   | undoable | {rect, cols, rows, gutter? (pt, 12), shape?, content?, spread?} — a grid of frames filling `rect` (frame tools: arrow keys while dragging) | cmd/object.rs:689 |
| `object.label` | Script Label | Window > Utilities > Script Label |   | undoable | {label, ids?} — free text scripts and agents can find objects by | cmd/object.rs:800 |
| `object.altText` | Object Export Options | Object |   | undoable | {text, ids?} — alternative text for tagged PDF and EPUB | cmd/object.rs:812 |
| `file.package` | Package… | File |   | noundo | {dir: folder to create, idml?: true, pdf?: false, instructions?: text} — the document (its placed files relinked to Links/), Links/, the font files that draw its text in Document Fonts/ (fallback fonts included; unless their licence restricts it), an IDML copy, an optional PDF and a report → {dir, files, report} | cmd/package.rs:195 |
| `links.copyTo` | Copy Link(s) To… |   |   | undoable | {dir, assets?: [asset ids] (default: all)} — write the placed files to `dir` and relink to the copies → {copied} | cmd/package.rs:204 |
| `preflight.run` | Preflight Document | Window > Output > Preflight |   | query | {minPpi?: 150} → issues [{severity, kind, message, item?, page?}] | cmd/preflight.rs:29 |
| `document.preferences` | Document Preferences |   |   | undoable | {horizontalUnits?, verticalUnits?: points\|picas\|inches\|millimeters\|…, keyboardIncrement? (pt), baselineGrid?: {start, increment, relativeTo, color, viewThreshold}, grid?: {horizontal, vertical, subdivisions, color, inBack}, pasteboard?: [h, v], marginColor?, columnColor?, bleedColor?, slugColor?: [r, g, b], advancedType?: {superscriptSize, superscriptPosition, subscriptSize, subscriptPosition} (%), overprintBlack?, glyphFallback? (draw characters the font lacks from fallback fonts; off: as the font's missing-glyph box)} → those settings | cmd/prefs.rs:60 |
| `file.printers` | Printers |   |   | query | {} → {printers: [names], default} | cmd/print.rs:14 |
| `file.print` | Print… | File | Cmd+P | noundo | {printer?, copies?: 1, pages?: "1-3,5", spreads?, marks?, bleed?, dryRun?: bool (export and preview submission without printing)} → {printer, copies, pages, command, bytes} | cmd/print.rs:15 |
| `script.run` | Run Script |   |   | undoable | {text} — command script lines (`command.id {json}`, JSON lines or a JSON array; `$N.path` refers to earlier results) → {results, steps} | cmd/scripts.rs:29 |
| `story.setText` | Set Story Text |   |   | undoable | {story, text} — replace a story's whole text | cmd/text.rs:90 |
| `story.get` | Get Story |   |   | query | {story? \| frame?} → text, frames, paragraphs, vertical, overset | cmd/text.rs:127 |
| `file.exportXml` | Export XML… | File |   | noundo | {path?} — the tagged content in reading order → {path, bytes} (no path: {text}) | cmd/xml.rs:303 |
| `file.importXml` | Import XML… | File |   | undoable | {path \| text} — each element's content goes into the next text frame with that tag (child elements of mapped tags become paragraphs in their style) → {placed, unmatched} | cmd/xml.rs:370 |

UI-layer file commands (app only):

| id | label | shortcut | params | src |
| --- | --- | --- | --- | --- |
| `app.openDialog` | Open… | Cmd+O | {} | menus.rs:10 |
| `app.placeDialog` | Place… | Cmd+D | {} | menus.rs:11 |
| `app.saveDialog` | Save As… | Cmd+Shift+S | {} | menus.rs:12 |
| `app.save` | Save | Cmd+S | {} | menus.rs:13 |
| `app.quit` | Quit DesignCraft | Cmd+Q | {} — close the window; each document with unsaved changes asks first | menus.rs:14 |
| `app.saveCopyDialog` | Save a Copy… | Cmd+Alt+S | {} — choose where to write a copy (the document stays as it is) | menus.rs:15 |
| `app.packageDialog` | Package… | Cmd+Alt+Shift+P | {} — choose the package folder to create (file.package) | menus.rs:17 |
| `app.saveSwatches` | Save Swatches for Exchange… |   | {path?} — write the colour swatches as .ase | menus.rs:18 |
| `app.exportPng` | Export Page as PNG… | Cmd+E | {} | menus.rs:19 |
| `app.exportIdml` | Export IDML… |   | {path?} — InDesign Markup (IDML) package | menus.rs:20 |
| `app.exportEpub` | Export EPUB… |   | {path?} — reflowable EPUB 3 (with the first page as its cover) | menus.rs:21 |
| `app.exportFixedEpub` | Export EPUB (Fixed Layout)… |   | {path?} — pre-paginated EPUB 3 | menus.rs:28 |
| `app.exportHtml` | Export HTML… |   | {path?} — one self-contained web page | menus.rs:29 |
| `app.exportXml` | Export XML… |   | {path?} — the tagged content | menus.rs:30 |
| `app.placeAndLink` | Place and Link |   | {} — a linked copy of the selected frame's story, beside it | menus.rs:51 |
| `app.placeWithOptions` | Place with Import Options… | Cmd+Shift+D | {} — Word/RTF style mapping before placing | menus.rs:52 |
| `app.exportText` | Export Text… |   | {path?} — the story being edited, as Text Only (.txt) or Rich Text Format (.rtf) | menus.rs:53 |
| `app.exportPdf` | Export PDF… |   | {path?, …file.exportPdf options} — asks for a path when none is given | menus.rs:54 |
| `window.nextDocument` | Next Document | Cmd+F6 | {} | menus.rs:127 |
| `window.newWindow` | New Window |   | {on?: bool} — another view of the active document in its own window | menus.rs:173 |

### 4. MCP server (`crates/mcp`)

| Aspect | Detail | src |
| --- | --- | --- |
| Start | `designcraft-cli mcp` (headless, with a document) or `designcraft-cli mcp --connect 7979` (running app); `--sample` | apps/designcraft-cli/src/main.rs:162-194 |
| Transport | JSON-RPC 2.0 over stdio; the remote backend speaks the control protocol | crates/mcp/src/lib.rs:5-10 |
| Protocol | `2025-06-18` (+ `2025-03-26`, `2024-11-05`) | crates/mcp/src/server.rs:11-12 |
| Auth | stdio, none (and the control port it bridges to has none) | — |
| Tools (24) | `list_commands`, `execute`, `batch`, `inspect_document`, `get_story`, `set_story_text`, `new_document`, `open_document` (.designcraft), `save_document`, **`place_image {path\|base64, name?, frame?, spread?, x?, y?, width?}`** (→ `file.place`), `render_page {page?, scale?, bleed?, path?}` (PNG), `export_png {path, page?, scale?}`, `select_tool`, `pointer`, `key`, `type_text`; app-only `screenshot`, `click`, `drag`, `menu_list`, `ui_inspect`, `ui_set`, `dialog_set`, `dialog_confirm`, `dialog_cancel` | crates/mcp/src/tools.rs:83-391 (definitions), 527-610 (`call_tool`) |
| Mirrors the registry? | Yes: `execute`/`batch` run any engine id (and UI ids when connected) | tools.rs:527-537 |
| Resources | `designcraft://document` (`document.inspect`) and commands (`engine.commands`); no templates | server.rs:154-171 |
| Prompts | None | server.rs:136 |

### 5. Control channel (running GUI)

| Aspect | Detail | src |
| --- | --- | --- |
| Enable | `designcraft --control 7979` (`DEFAULT_PORT = 7979` in the MCP crate) | crates/mcp/src/lib.rs:29; apps/designcraft/src/control_server.rs:26-37 |
| Protocol | JSON lines: `{"id","method","params"}` → `{"id","ok":true,"result"}` / `{"ok":false,"error"}`; a non-request line gets one error and the connection closes | control_server.rs:1-8; docs/control-protocol.md:17-30 |
| Auth | **None.** "The port has no authentication, so only enable it while you use it." Only the line shape is checked, to block cross-origin `fetch`. Max 4 MiB lines and 16 connections; 60 s reply timeout | docs/control-protocol.md:26-27; control_server.rs:20-24, 58-64 |
| Methods | `engine.execute`/`ui.menu.invoke`/`command {command, params}`, `engine.commands`, `document.inspect`, `ui.inspect`, `ui.menu.list`, `ui.tool.select`, `ui.tool.list`, `ui.pointer`, `ui.key`, `ui.text`, `ui.move`, `ui.click`, `ui.drag`, `ui.set`, `ui.dialog.open/set/confirm/cancel`, `ui.resize`, `ui.focus`, `ui.screenshot {path?}`, `ui.render {path?, page?, scale?, bleed?}`, **`app.open {path}`**, `app.save {path?}`, **`app.export {path, page?, scale?}`** (PNG), `app.quit` | crates/ui-egui/src/control.rs:1-19, 122-382; docs/control-protocol.md:32-50 |
| Targets the running GUI? | Yes; documents open as new active tabs. The same requests also run headless (crates/mcp/src/headless.rs:234-254) | — |

```json
{"id":1,"method":"app.open","params":{"path":"/abs/layout.designcraft"}}
{"id":2,"method":"engine.execute","params":{"command":"frame.create","params":{"rect":[36,36,300,200],"content":"text"}}}
{"id":2,"ok":true,"result":{"id":10,"story":11}}
```

### 6. Library crates (no UI deps)

All are MIT OR Apache-2.0, **path-only with no `version`** in the workspace (Cargo.toml:16-31), so they are git deps at `v0.8.0`. `craft-print` (git) is pulled in by `designcraft-engine`.

| Crate | Layer | Purpose | Notable deps |
| --- | --- | --- | --- |
| `designcraft-geom`, `designcraft-color` | L0 | geometry; colour/ICC (moxcms) | kurbo, flo_curves, qrcodegen; moxcms |
| `designcraft-doc` | L1 | document model (spreads, pages, items, stories, styles) | serde |
| `designcraft-fonts` | L1 | font DB, shaping | skrifa 0.47, harfrust 0.13 |
| `designcraft-compose` | L2 | text composer; `Cache` (`Default`) (crates/compose/src/cache.rs:26-36) | hypher, regex |
| `designcraft-images` | L2 | placed-graphic decode: PNG, JPEG, GIF, WebP, TIFF (CMYK), BMP, PSD, SVG | image, tiff, resvg |
| **`designcraft-format`** | L3 | `.designcraft` zip: `load(&[u8]) -> Document`, `save(&Document)` (crates/format/src/lib.rs:1-9, 50, 83) | zip, serde_json |
| **`designcraft-idml`** | L3 | `import_idml(bytes)`, `import_idml_with(bytes, read_link)`, `export_idml(doc)` (crates/idml/src/import.rs:33-64; export.rs:42-47) | quick-xml, zip |
| **`designcraft-render`** | L3 | `Renderer::new().render_page(&doc, &cache, page, scale, bleed, &RenderOptions) -> Option<Rendered>` (crates/render/src/lib.rs:214, 259, 351); placed PDFs through hayro | vello_cpu 0.2, hayro 0.7 (patched), image |
| `designcraft-pdf` | L3 | PDF export (krilla), PDF/X-4 checks | krilla 0.8, hayro-syntax |
| `designcraft-epub`, `designcraft-textimport` | L3 | EPUB export; docx/rtf/xlsx/tagged text import | zip, quick-xml |
| `designcraft-tools` | L4 | pointer → command state machines | |
| `designcraft-engine` | L5 | `Session`, the 459 commands, script runner (`script::parse/resolve/run`) | everything above, craft-print |
| `designcraft-mcp` | L6 | MCP server + `Headless`/`Remote` backends | engine, render |

**`.designcraft` / `.idml` page thumbnail.** Load with `designcraft_format::load(&bytes)` (or `designcraft_idml::import_idml(&bytes)`), then call `designcraft_render::Renderer::new().render_page(&doc, &designcraft_compose::Cache::default(), 0, scale, false, &RenderOptions { printing_only: true, ..Default::default() })` and `.to_png()`. This is what the CLI `export()` does, except that the CLI passes `bleed = true` (apps/designcraft-cli/src/main.rs:308-316). The `.designcraft` zip carries no embedded preview image (`mimetype`, `document.json`, `assets/`, `meta.json` only; crates/format/src/lib.rs:1-9).

### 7. File formats

| Direction | Formats | src |
| --- | --- | --- |
| Native | `.designcraft` (zip: `mimetype`, `document.json`, `assets/<id>.<ext>`, `meta.json`; format v2); `.dcbook` books; `.dclib` object libraries; snippets (`.designcraft`) | crates/format/src/lib.rs:1-23; cmd/book.rs:241; cmd/library.rs:2 |
| Open | `.designcraft`, `.idml` (+ `Document Fonts/` folder, `Links/`), `file.openBytes` | cmd/file.rs:20-23; cmd/interchange.rs:15 |
| Place | PNG, JPEG, GIF, WebP, TIFF, BMP, PSD, SVG, EPS preview, **PDF** (`pdfPage`, `pdfCrop`), `.designcraft`/IDML pages (`layoutPage`); text .txt/.docx/.rtf/.md; .xlsx as a table | cmd/file.rs:100-104; docs/architecture.md:22 |
| Export | PDF (print/interactive, PDF/X-4, PDF/A-2b), IDML, EPUB reflowable and fixed, HTML, XML, text/RTF/Tagged Text, PNG/JPEG (CLI, MCP, UI only), Package (folder with Links/, fonts, IDML, PDF), booklet PDF | cmd/export.rs:11-28; cmd/package.rs:195 |
| Templates | `file.new {preset, width, height, pages, facingPages, columns, margins, bleed}`, `file.presets`; parent pages; Data Merge templates; object libraries | cmd/file.rs:15-19 |

### 8. Opportunities for AssetCraft (DesignCraft)

| AssetCraft wants | Surface that supports it today | Opportunity / suggested fix | Owner |
| --- | --- | --- | --- |
| Open file(s) in the running instance | Control `app.open {path}` → a new active document; `designcraft-cli app --port 7979 file.open '{"path":…}'` | (1) **The control port is unauthenticated** (docs/control-protocol.md:26-27). Adopt the WordCraft key-file or PdfCraft token scheme before AssetCraft relies on it. (2) No single-instance hand-off: a second `designcraft f.idml` starts a second process. Add PdfCraft-style hand-off | DesignCraft |
| **Send a selection as a contact sheet or layout** | No one-shot command. Composable today, headless or live via `script --connect`: `file.new {preset}` → `frame.grid {rect, cols, rows, gutter, content:"graphic"}` (returns `{ids}`, cmd/object.rs:689-714) → `file.place {path, frame: "$1.ids.N"}` per image → `object.fit {mode: fitProportionally}` → optional `frame.create` captions → `file.saveAs` / `--export sheet.pdf`. Alternative: Data Merge with an image placeholder and `perPage`/`arrange` (`data.source.select` CSV of paths → `data.placeholder.add {role:"image"}` → `data.merge {perPage…}`; cmd/datamerge/mod.rs:31-72) | Add `layout.contactSheet {paths[], cols, rows, gutter, captions?: filename\|none, fit, preset}` so AssetCraft sends one command (and N > cols×rows adds pages) | DesignCraft (AssetCraft can ship the script meanwhile) |
| Page thumbnail, headless | CLI `run --in f.designcraft --scale 0.25 --export t.png`; library route in §6; MCP `render_page` | Write `preview.png` (first page) into the `.designcraft` zip on save so browsers can thumbnail without composing text | DesignCraft |
| Text and metadata for search | `document.inspect`, `story.get`/MCP `get_story`, `file.exportText`; library: `designcraft_format::load` → walk `doc.stories` | Add a `document.text` query (all stories in reading order) and title/author metadata in `meta.json` | DesignCraft |
| Export | `run --export .pdf/.idml/.epub/.png`; `file.exportPdf`/`exportIdml`/`exportEpub`/`exportHtml`; `file.package` | PNG export has no engine command (CLI/MCP/UI call `designcraft-render` directly: apps/designcraft-cli/src/main.rs:308-316; UI-only `app.exportPng`). Add an engine `file.exportPng` so `script` can do it | DesignCraft |
| Place PDFs from PdfCraft | `file.place {path, pdfPage, pdfCrop}` | Works today (hayro from PdfCraft) | — |
| Depend on crates | path-only versions | Add `version = "0.8.0"` to workspace deps | DesignCraft |

---

## Cross-app notes for AssetCraft

- **Suggested surface for each job:**

  | Job | Surface |
  | --- | --- |
  | PDF thumbnails, text and page sizes | Link `pdfcraft-render` + `pdfcraft-cos`/`pdfcraft-model` |
  | DOCX/IDML/`.designcraft` thumbnails | Either link the crates (git deps) or spawn the CLIs (`wordcraft-cli render`, `designcraft-cli run --export`); spawning keeps AssetCraft's dependency graph smaller |
  | Hand files to a running app | Use the control channels: PdfCraft token file, WordCraft key file, DesignCraft (unauthenticated, port 7979 by convention) |

- **Control channel shapes differ.**

  | App | Shape |
  | --- | --- |
  | PdfCraft | JSON-RPC 2.0 with an `auth` first request, on a random port |
  | WordCraft | `{id,key,method,params}`, or `auth` once per connection |
  | DesignCraft | `{id,method,params}` with no auth |

  A small `craft-control` client crate in craft-libs, with a unified handshake, would let AssetCraft talk to all three the same way (owner: craft-libs).

- **MCP:** all three servers are stdio-only, opt-in, and without prompts. WordCraft and DesignCraft can bridge to the running GUI (`--connect`); PdfCraft MCP is headless only (its GUI is reachable only through `pdfcraft-cli ui`).
