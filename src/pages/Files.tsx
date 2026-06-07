import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { php } from "@codemirror/lang-php";
import { sql } from "@codemirror/lang-sql";
import { EditorView } from "@codemirror/view";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import CodeMirror from "@uiw/react-codemirror";
import {
  Archive,
  CheckSquare,
  Copy,
  Edit,
  ExternalLink,
  FilePlus,
  FileText,
  Folder,
  FolderOpen,
  FolderPlus,
  MoveRight,
  RefreshCw,
  Replace,
  RotateCcw,
  Save,
  Search,
  Shield,
  Trash2,
  Upload
} from "lucide-react";
import { useEffect, useMemo, useState, type MouseEvent } from "react";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { api } from "../ui/api";
import { useT } from "../ui/i18n";
import type { AppRun, AppSnapshot, ConfigFile, FileEntry, FileSearchResult } from "../ui/types";

type PaneKey = "left" | "right";
type PaneState = {
  folder: string;
  entries: FileEntry[];
  selected: FileEntry | null;
  filter: string;
};

type FileOperation = {
  id: string;
  label: string;
  status: "running" | "done" | "error" | "cancelled";
  message?: string;
};

type FileContextMenu = {
  x: number;
  y: number;
  pane: PaneKey;
  entry: FileEntry;
};

const encodings = ["auto", "utf-8", "utf-8-bom", "utf-16le", "utf-16be"];

export function FilesPage({ state, run }: { state: AppSnapshot; run: AppRun }) {
  const t = useT();
  const roots = useMemo(() => [
    { label: "Projects", path: state.settings.projectsFolder },
    { label: "App Data", path: state.appDataDir },
    { label: "Services", path: state.settings.servicesFolder },
    { label: "Backups", path: state.settings.backupsFolder },
    ...state.hosts.slice(0, 8).map((host) => ({ label: host.domain, path: host.rootFolder }))
  ], [state.appDataDir, state.hosts, state.settings.backupsFolder, state.settings.projectsFolder, state.settings.servicesFolder]);

  const [activePane, setActivePane] = useState<PaneKey>("left");
  const [leftPane, setLeftPane] = useState<PaneState>(() => paneState(roots[0]?.path ?? state.appDataDir));
  const [rightPane, setRightPane] = useState<PaneState>(() => paneState(state.appDataDir));
  const [openedFile, setOpenedFile] = useState<ConfigFile | null>(null);
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [nameInput, setNameInput] = useState("");
  const [targetPath, setTargetPath] = useState("");
  const [archivePath, setArchivePath] = useState("");
  const [chmodMode, setChmodMode] = useState("644");
  const [overwrite, setOverwrite] = useState(false);
  const [encoding, setEncoding] = useState("auto");
  const [editorSearch, setEditorSearch] = useState("");
  const [editorReplace, setEditorReplace] = useState("");
  const [editorRegexp, setEditorRegexp] = useState(false);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wrapLines, setWrapLines] = useState(true);
  const [editorMessage, setEditorMessage] = useState("");
  const [contentSearch, setContentSearch] = useState("");
  const [contentResults, setContentResults] = useState<FileSearchResult[]>([]);
  const [selectedPaths, setSelectedPaths] = useState<Record<PaneKey, string[]>>({ left: [], right: [] });
  const [operationQueue, setOperationQueue] = useState<FileOperation[]>([]);
  const [contextMenu, setContextMenu] = useState<FileContextMenu | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);

  const dirty = openedFile ? content !== savedContent : false;
  const pane = activePane === "left" ? leftPane : rightPane;
  const otherPane = activePane === "left" ? rightPane : leftPane;
  const selected = pane.selected;
  const selectedPath = selected?.path ?? openedFile?.path ?? pane.folder;
  const selectedEntries = useMemo(() => {
    const paths = new Set(selectedPaths[activePane]);
    const entries = pane.entries.filter((entry) => paths.has(entry.path));
    return entries.length > 0 ? entries : selected ? [selected] : [];
  }, [activePane, pane.entries, selected, selectedPaths]);
  const selectedFilesLabel = selectedEntries.length > 1 ? `${selectedEntries.length} files` : selectedEntries[0]?.name ?? "";
  const editorMatches = useMemo(() => countMatches(content, editorSearch, editorRegexp, caseSensitive), [caseSensitive, content, editorRegexp, editorSearch]);

  const setPane = (key: PaneKey, next: PaneState | ((current: PaneState) => PaneState)) => {
    if (key === "left") setLeftPane(next);
    else setRightPane(next);
  };

  const setPaneSelectedPaths = (key: PaneKey, paths: string[]) => {
    setSelectedPaths((current) => ({ ...current, [key]: paths }));
  };

  const runFileOperation = async (label: string, action: () => Promise<void>) => {
    const id = crypto.randomUUID();
    const operation: FileOperation = { id, label, status: "running" };
    setOperationQueue((current) => [operation, ...current].slice(0, 8));
    try {
      await action();
      setOperationQueue((current) => current.map((item) => item.id === id ? { ...item, status: "done" } : item));
    } catch (error) {
      setOperationQueue((current) => current.map((item) => item.id === id ? { ...item, status: "error", message: String(error) } : item));
    }
  };

  const refresh = async (key: PaneKey = activePane, target?: string) => {
    const current = key === "left" ? leftPane : rightPane;
    const folder = target ?? current.folder;
    const result = await run(() => api.listFiles(folder), { label: `Reading ${folder}...` });
    if (Array.isArray(result)) {
      setPane(key, { ...current, folder, entries: result as FileEntry[], selected: null });
      setPaneSelectedPaths(key, []);
    }
  };

  const selectEntry = (key: PaneKey, entry: FileEntry, event?: MouseEvent) => {
    setActivePane(key);
    const current = key === "left" ? leftPane : rightPane;
    setPane(key, { ...current, selected: entry });
    const currentPaths = selectedPaths[key];
    if (event?.ctrlKey || event?.metaKey) {
      setPaneSelectedPaths(key, currentPaths.includes(entry.path) ? currentPaths.filter((path) => path !== entry.path) : [...currentPaths, entry.path]);
    } else if (event?.shiftKey && current.selected) {
      const entries = current.entries;
      const start = entries.findIndex((item) => item.path === current.selected?.path);
      const end = entries.findIndex((item) => item.path === entry.path);
      if (start >= 0 && end >= 0) {
        const [from, to] = start < end ? [start, end] : [end, start];
        setPaneSelectedPaths(key, entries.slice(from, to + 1).map((item) => item.path));
      }
    } else {
      setPaneSelectedPaths(key, [entry.path]);
    }
  };

  const openEntry = async (key: PaneKey, entry: FileEntry) => {
    setActivePane(key);
    selectEntry(key, entry);
    setEditorMessage("");
    if (entry.kind === "folder") {
      if (dirty) {
        setEditorMessage("Save or reload the current file before opening another file.");
        return;
      }
      await refresh(key, entry.path);
      return;
    }
    if (dirty && entry.path !== openedFile?.path) {
      setEditorMessage("Save or reload the current file before opening another file.");
      return;
    }
    await openFile(entry.path, encoding, true);
  };

  const openFile = async (path: string, fileEncoding = encoding, showModal = false) => {
    const result = await run(() => api.readFileWithEncoding(path, fileEncoding), { label: `Opening ${fileName(path)}...` });
    if (result && typeof result === "object" && "content" in result) {
      const file = result as ConfigFile;
      setOpenedFile(file);
      setContent(file.content);
      setSavedContent(file.content);
      setEncoding(file.encoding ?? fileEncoding);
      if (showModal) setEditorOpen(true);
    }
  };

  const createFolder = async () => {
    const name = nameInput.trim();
    if (!name) return;
    await run(() => api.createFolder(joinPath(pane.folder, name)), { label: `Creating ${name}...` });
    setNameInput("");
    await refresh(activePane);
  };

  const createFile = async () => {
    const name = nameInput.trim();
    if (!name) return;
    const path = joinPath(pane.folder, name);
    await run(() => api.createFile(path), { label: `Creating ${name}...` });
    setNameInput("");
    await refresh(activePane);
    await openFile(path, "utf-8");
  };

  const renameSelected = async () => {
    const name = nameInput.trim();
    if (!selected || !name) return;
    const result = await run(() => api.renamePath(selected.path, name), { label: `Renaming ${selected.name}...` });
    if (typeof result === "string" && openedFile?.path === selected.path) {
      setOpenedFile({ ...openedFile, path: result });
    }
    setNameInput("");
    await refresh(activePane);
  };

  const copySelected = async (toOtherPane = false) => {
    if (!selectedEntries.length) return;
    const target = targetPath.trim() || (toOtherPane ? otherPane.folder : pane.folder);
    await runFileOperation(`Copy ${selectedFilesLabel}`, async () => {
      for (const item of selectedEntries) {
        await run(() => api.copyPath(item.path, target, overwrite), { label: `Copying ${item.name}...`, silent: selectedEntries.length > 1 });
      }
      await refresh(activePane);
      await refresh(activePane === "left" ? "right" : "left");
    });
  };

  const moveSelected = async (toOtherPane = false) => {
    if (!selectedEntries.length) return;
    const target = targetPath.trim() || (toOtherPane ? otherPane.folder : pane.folder);
    await runFileOperation(`Move ${selectedFilesLabel}`, async () => {
      for (const item of selectedEntries) {
        await run(() => api.movePath(item.path, target, overwrite), { label: `Moving ${item.name}...`, silent: selectedEntries.length > 1 });
      }
      if (openedFile && selectedEntries.some((item) => item.path === openedFile.path)) setOpenedFile(null);
      await refresh(activePane);
      await refresh(activePane === "left" ? "right" : "left");
    });
  };

  const deleteSelected = async () => {
    if (!selectedEntries.length) return;
    await runFileOperation(`Delete ${selectedFilesLabel}`, async () => {
      for (const item of selectedEntries) {
        await run(() => api.deletePath(item.path), { label: `Deleting ${item.name}...`, silent: selectedEntries.length > 1 });
      }
      if (openedFile && selectedEntries.some((item) => item.path === openedFile.path || item.kind === "folder")) closeEditor();
      await refresh(activePane);
    });
  };

  const chmodSelected = async () => {
    if (!selectedEntries.length) return;
    const readOnly = ["400", "440", "444", "500", "550", "555"].includes(chmodMode.trim());
    await runFileOperation(`chmod ${selectedFilesLabel}`, async () => {
      for (const item of selectedEntries) {
        await run(() => api.chmodPath(item.path, chmodMode, readOnly), { label: `Changing permissions for ${item.name}...`, silent: selectedEntries.length > 1 });
      }
      await refresh(activePane);
      if (openedFile && selectedEntries.some((item) => item.path === openedFile.path)) await openFile(openedFile.path, encoding);
    });
  };

  const uploadIntoPane = async () => {
    const selectedFiles = await openDialog({ multiple: true, directory: false });
    const sources = Array.isArray(selectedFiles) ? selectedFiles : selectedFiles ? [selectedFiles] : [];
    if (!sources.length) return;
    await run(() => api.uploadFiles(sources, pane.folder, overwrite), { label: `Uploading ${sources.length} file(s)...` });
    await refresh(activePane);
  };

  const uploadDroppedFiles = async (key: PaneKey, files: FileList) => {
    const destination = key === "left" ? leftPane.folder : rightPane.folder;
    const sources = Array.from(files)
      .map((file) => (file as File & { path?: string }).path)
      .filter((path): path is string => Boolean(path));
    if (!sources.length) return;
    await runFileOperation(`Upload ${sources.length} file(s)`, async () => {
      await run(() => api.uploadFiles(sources, destination, overwrite), { label: `Uploading ${sources.length} file(s)...` });
      await refresh(key);
    });
  };

  const extractSelected = async () => {
    if (!selectedEntries.length) return;
    const archives = selectedEntries.filter((item) => item.kind === "file");
    await runFileOperation(`Extract ${archives.length} archive(s)`, async () => {
      for (const item of archives) {
        await run(() => api.extractArchiveTo(item.path, targetPath.trim() || pane.folder), { label: `Extracting ${item.name}...`, silent: archives.length > 1 });
      }
      await refresh(activePane);
      await refresh(activePane === "left" ? "right" : "left");
    });
  };

  const createArchive = async () => {
    if (!selectedEntries.length) return;
    let target = archivePath.trim();
    if (!target) {
      const baseName = selectedEntries.length === 1 ? stripExtension(selectedEntries[0].name) : "selected-files";
      const saved = await saveDialog({ defaultPath: joinPath(pane.folder, `${baseName}.zip`) });
      if (!saved) return;
      target = saved;
    }
    const paths = selectedEntries.map((item) => item.path);
    await runFileOperation(`Archive ${selectedFilesLabel}`, async () => {
      await run(() => api.createArchive(paths, target), { label: `Creating archive ${fileName(target)}...` });
      setArchivePath(target);
      await refresh(activePane);
    });
  };

  const saveFile = async () => {
    if (!openedFile || openedFile.readOnly) return;
    await run(() => api.writeFileWithEncoding(openedFile.path, content, encoding), { label: `Saving ${openedFile.path}...` });
    setSavedContent(content);
    setEditorMessage("Saved.");
    await refresh("left");
    await refresh("right");
  };

  const reloadFile = async () => {
    if (!openedFile) return;
    await openFile(openedFile.path, encoding);
    setEditorMessage("");
  };

  const replaceNext = () => {
    const next = replaceInText(content, editorSearch, editorReplace, editorRegexp, caseSensitive, false);
    setContent(next);
  };

  const replaceAll = () => {
    const next = replaceInText(content, editorSearch, editorReplace, editorRegexp, caseSensitive, true);
    setContent(next);
  };

  const searchContents = async () => {
    if (!contentSearch.trim()) {
      setContentResults([]);
      return;
    }
    const result = await run(() => api.searchFileContents(pane.folder, contentSearch, editorRegexp, caseSensitive), { label: `Searching ${pane.folder}...` });
    if (Array.isArray(result)) setContentResults(result as FileSearchResult[]);
  };

  const copyPath = (value: string) => {
    void navigator.clipboard?.writeText(value);
    setEditorMessage("Path copied.");
  };

  const openContextMenu = (key: PaneKey, entry: FileEntry, event: MouseEvent) => {
    event.preventDefault();
    selectEntry(key, entry, event);
    setContextMenu({ pane: key, entry, x: event.clientX, y: event.clientY });
  };

  const editEntry = async (entry = selectedEntries[0]) => {
    if (!entry || entry.kind !== "file") return;
    setContextMenu(null);
    await openFile(entry.path, encoding, true);
  };

  const closeEditor = () => {
    setOpenedFile(null);
    setContent("");
    setSavedContent("");
    setEditorOpen(false);
  };

  useEffect(() => {
    void refresh("left", roots[0]?.path ?? state.appDataDir);
    void refresh("right", state.appDataDir);
  }, [state.appDataDir]);

  return (
    <div className="files-workspace">
      <div className="page-title">
        <div><h1>{t("File Manager")}</h1><p>{t("Project files, configs, logs and backups")}</p></div>
        <div className="toolbar files-main-toolbar">
          <input className="toolbar-input" value={nameInput} onChange={(event) => setNameInput(event.target.value)} placeholder={String(t("File or folder name"))} />
          <Button icon={<FolderPlus size={16} />} disabled={!nameInput.trim()} onClick={() => void createFolder()}>New Folder</Button>
          <Button icon={<FilePlus size={16} />} disabled={!nameInput.trim()} onClick={() => void createFile()}>New File</Button>
          <Button icon={<Upload size={16} />} onClick={() => void uploadIntoPane()}>Upload</Button>
        </div>
      </div>

      <Panel title="Files">
        <div className="dual-file-manager">
          <FilePane paneKey="left" pane={leftPane} active={activePane === "left"} selectedPaths={selectedPaths.left} roots={roots} onActivate={() => setActivePane("left")} onRefresh={(path) => void refresh("left", path)} onFilter={(filter) => setLeftPane((current) => ({ ...current, filter }))} onSelect={selectEntry} onOpen={(entry) => void openEntry("left", entry)} onContextMenu={openContextMenu} onDropFiles={uploadDroppedFiles} onCopyPath={copyPath} />
          <FilePane paneKey="right" pane={rightPane} active={activePane === "right"} selectedPaths={selectedPaths.right} roots={roots} onActivate={() => setActivePane("right")} onRefresh={(path) => void refresh("right", path)} onFilter={(filter) => setRightPane((current) => ({ ...current, filter }))} onSelect={selectEntry} onOpen={(entry) => void openEntry("right", entry)} onContextMenu={openContextMenu} onDropFiles={uploadDroppedFiles} onCopyPath={copyPath} />
        </div>
      </Panel>

      <div className="file-tools-grid">
        <Panel title="Operations">
          <div className="file-ops-grid">
            <input className="toolbar-input" value={targetPath} onChange={(event) => setTargetPath(event.target.value)} placeholder={String(t("Absolute target path"))} />
            <label className="check-line"><input type="checkbox" checked={overwrite} onChange={(event) => setOverwrite(event.target.checked)} /> {t("Overwrite")}</label>
            <Button disabled={!selectedEntries.length} icon={<Copy size={16} />} onClick={() => void copySelected(false)}>Copy</Button>
            <Button disabled={!selectedEntries.length} icon={<MoveRight size={16} />} onClick={() => void moveSelected(false)}>Move</Button>
            <Button disabled={!selectedEntries.length} icon={<Copy size={16} />} onClick={() => void copySelected(true)}>Copy to Other Pane</Button>
            <Button disabled={!selectedEntries.length} icon={<MoveRight size={16} />} onClick={() => void moveSelected(true)}>Move to Other Pane</Button>
            <Button disabled={!selectedEntries.length || !nameInput.trim()} onClick={() => void renameSelected()}>Rename</Button>
            <Button disabled={!selectedEntries.length} icon={<Trash2 size={16} />} variant="danger" onClick={() => void deleteSelected()}>Delete</Button>
          </div>
        </Panel>

        <Panel title="Permissions and Archives">
          <div className="file-ops-grid compact">
            <input className="toolbar-input" value={chmodMode} onChange={(event) => setChmodMode(event.target.value)} placeholder="644" />
            <Button disabled={!selectedEntries.length} icon={<Shield size={16} />} onClick={() => void chmodSelected()}>Apply chmod</Button>
            <input className="toolbar-input" value={archivePath} onChange={(event) => setArchivePath(event.target.value)} placeholder={String(t("Archive path"))} />
            <Button disabled={!selectedEntries.length} icon={<Archive size={16} />} onClick={() => void createArchive()}>Create Archive</Button>
            <Button disabled={!selectedEntries.length} icon={<Archive size={16} />} onClick={() => void extractSelected()}>Extract Here</Button>
            <Button icon={<FolderOpen size={16} />} onClick={() => void run(() => api.openPath(pane.folder), { label: `Opening ${pane.folder}...` })}>Open Folder</Button>
          </div>
        </Panel>
      </div>

      <div className="files-editor-grid">
        <Panel title="Operation Queue" action={<Button icon={<Trash2 size={16} />} onClick={() => setOperationQueue([])}>Clear</Button>}>
          <div className="file-operation-list">
            <div className="selection-summary">
              <CheckSquare size={18} />
              <strong>{selectedEntries.length}</strong>
              <span>{t("selected")}</span>
            </div>
            {operationQueue.map((operation) => (
              <div className={`file-operation file-operation-${operation.status}`} key={operation.id}>
                <i />
                <strong>{t(operation.label)}</strong>
                <span>{t(operation.status)}</span>
                {operation.status === "running" && <button onClick={() => setOperationQueue((items) => items.map((item) => item.id === operation.id ? { ...item, status: "cancelled" } : item))}>{t("Cancel")}</button>}
              </div>
            ))}
            {!operationQueue.length && <div className="empty-row">{t("No file operations yet.")}</div>}
          </div>
        </Panel>

        <Panel title="Content Search">
          <div className="content-search-box">
            <div className="file-filter">
              <Search size={16} />
              <input value={contentSearch} onChange={(event) => setContentSearch(event.target.value)} placeholder={String(t("Search in files..."))} />
              <span>{contentResults.length}</span>
            </div>
            <div className="toolbar">
              <label className="check-line"><input type="checkbox" checked={editorRegexp} onChange={(event) => setEditorRegexp(event.target.checked)} /> RegExp</label>
              <label className="check-line"><input type="checkbox" checked={caseSensitive} onChange={(event) => setCaseSensitive(event.target.checked)} /> Aa</label>
              <Button icon={<Search size={16} />} onClick={() => void searchContents()}>Search</Button>
            </div>
            <div className="content-results">
              {contentResults.map((result) => (
                <button key={`${result.path}:${result.line}:${result.column}`} onClick={() => void openFile(result.path, encoding)} title={result.path}>
                  <strong>{fileName(result.path)}:{result.line}:{result.column}</strong>
                  <span>{result.preview}</span>
                </button>
              ))}
              {!contentResults.length && <div className="empty-row">{t("No matches found.")}</div>}
            </div>
          </div>
        </Panel>
      </div>

      {contextMenu && (
        <div className="file-context-menu" style={{ left: contextMenu.x, top: contextMenu.y }} onMouseLeave={() => setContextMenu(null)}>
          <button disabled={contextMenu.entry.kind !== "file"} onClick={() => void editEntry(contextMenu.entry)}><Edit size={15} />{t("Edit")}</button>
          <button onClick={() => copyPath(contextMenu.entry.path)}><Copy size={15} />{t("Copy Path")}</button>
          <button onClick={() => void copySelected(true)}><Copy size={15} />{t("Copy to Other Pane")}</button>
          <button onClick={() => void moveSelected(true)}><MoveRight size={15} />{t("Move to Other Pane")}</button>
          <button className="danger" onClick={() => void deleteSelected()}><Trash2 size={15} />{t("Delete")}</button>
        </div>
      )}

      {editorOpen && openedFile && (
        <div className="modal-backdrop code-modal-backdrop" onMouseDown={(event) => {
          if (event.target === event.currentTarget && !dirty) setEditorOpen(false);
        }}>
          <div className="code-modal">
            <div className="code-modal-head">
              <div>
                <h2>{t("Code Editor")} - {fileName(openedFile.path)}</h2>
                <code>{openedFile.path}</code>
              </div>
              <div className="toolbar">
                <select className="toolbar-input compact-select" value={encoding} onChange={(event) => {
                  setEncoding(event.target.value);
                  if (!dirty) void openFile(openedFile.path, event.target.value, true);
                }}>
                  {encodings.map((item) => <option key={item} value={item}>{item}</option>)}
                </select>
                <Button icon={<RotateCcw size={16} />} onClick={() => void reloadFile()}>Reload</Button>
                <Button icon={<ExternalLink size={16} />} onClick={() => void run(() => api.openPath(openedFile.path), { label: `Opening ${openedFile.path}...` })}>Open</Button>
                <Button variant="primary" icon={<Save size={16} />} disabled={!dirty || openedFile.readOnly} onClick={() => void saveFile()}>Save</Button>
                <Button onClick={() => setEditorOpen(false)}>Close</Button>
              </div>
            </div>
            <div className="editor-findbar">
              <input className="toolbar-input" value={editorSearch} onChange={(event) => setEditorSearch(event.target.value)} placeholder={String(t("Find"))} />
              <input className="toolbar-input" value={editorReplace} onChange={(event) => setEditorReplace(event.target.value)} placeholder={String(t("Replace"))} />
              <label className="check-line"><input type="checkbox" checked={editorRegexp} onChange={(event) => setEditorRegexp(event.target.checked)} /> RegExp</label>
              <label className="check-line"><input type="checkbox" checked={caseSensitive} onChange={(event) => setCaseSensitive(event.target.checked)} /> Aa</label>
              <label className="check-line"><input type="checkbox" checked={wrapLines} onChange={(event) => setWrapLines(event.target.checked)} /> {t("Wrap")}</label>
              <Button icon={<Replace size={16} />} disabled={!editorSearch} onClick={replaceNext}>Replace</Button>
              <Button icon={<Replace size={16} />} disabled={!editorSearch} onClick={replaceAll}>Replace All</Button>
            </div>
            <CodeMirror
              value={content}
              height="calc(100vh - 260px)"
              extensions={[languageExtension(openedFile.language ?? openedFile.path), editorTheme, wrapLines ? EditorView.lineWrapping : []]}
              basicSetup={{ searchKeymap: true, foldGutter: true, highlightActiveLine: true, highlightSelectionMatches: true }}
              editable={!openedFile.readOnly}
              onChange={(value) => {
                setContent(value);
                setEditorMessage("");
              }}
            />
            <div className="file-editor-status">
              <span>{dirty ? t("Unsaved changes") : t("No unsaved changes")} · {openedFile.language ?? "Text"} · {openedFile.encoding ?? encoding} · {formatSize(openedFile.size ?? content.length)} · {editorMatches} {t("matches")}</span>
              {editorMessage && <strong>{t(editorMessage)}</strong>}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function FilePane({
  paneKey,
  pane,
  active,
  selectedPaths,
  roots,
  onActivate,
  onRefresh,
  onFilter,
  onSelect,
  onOpen,
  onContextMenu,
  onDropFiles,
  onCopyPath
}: {
  paneKey: PaneKey;
  pane: PaneState;
  active: boolean;
  selectedPaths: string[];
  roots: Array<{ label: string; path: string }>;
  onActivate: () => void;
  onRefresh: (path?: string) => void;
  onFilter: (filter: string) => void;
  onSelect: (pane: PaneKey, entry: FileEntry, event?: MouseEvent) => void;
  onOpen: (entry: FileEntry) => void;
  onContextMenu: (pane: PaneKey, entry: FileEntry, event: MouseEvent) => void;
  onDropFiles: (pane: PaneKey, files: FileList) => void;
  onCopyPath: (path: string) => void;
}) {
  const t = useT();
  const filteredEntries = useMemo(() => {
    const query = pane.filter.trim().toLowerCase();
    if (!query) return pane.entries;
    return pane.entries.filter((entry) => entry.name.toLowerCase().includes(query));
  }, [pane.entries, pane.filter]);

  return (
    <div
      className={`file-pane ${active ? "active" : ""}`}
      onFocus={onActivate}
      onClick={onActivate}
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        onActivate();
        if (event.dataTransfer.files.length) onDropFiles(paneKey, event.dataTransfer.files);
      }}
    >
      <div className="file-pane-head">
        <strong>{paneKey === "left" ? t("Left Pane") : t("Right Pane")}</strong>
        <Button variant="icon" icon={<RefreshCw size={16} />} onClick={() => onRefresh()} aria-label="Refresh" />
      </div>
      <div className="file-pathbar">
        <button onClick={() => onRefresh(parentFolder(pane.folder))}>..</button>
        <code>{pane.folder}</code>
        <button title={String(t("Copy Path"))} onClick={() => onCopyPath(pane.folder)}><Copy size={15} /></button>
      </div>
      <div className="file-roots compact-roots">
        {roots.map((root) => (
          <button key={`${paneKey}-${root.label}`} className={isInside(pane.folder, root.path) ? "active" : ""} onClick={() => onRefresh(root.path)} title={root.path}>
            <Folder size={16} />
            <span>{root.label}</span>
          </button>
        ))}
      </div>
      <div className="file-filter">
        <Search size={16} />
        <input value={pane.filter} onChange={(event) => onFilter(event.target.value)} placeholder={String(t("Search files..."))} />
        <span>{filteredEntries.length} / {pane.entries.length}</span>
      </div>
      <div className="file-list pane-list">
        {filteredEntries.map((entry) => (
          <button
            key={entry.path}
            className={`file-row ${selectedPaths.includes(entry.path) ? "selected" : ""}`}
            onClick={(event) => onSelect(paneKey, entry, event)}
            onDoubleClick={() => onOpen(entry)}
            onContextMenu={(event) => onContextMenu(paneKey, entry, event)}
            title={entry.path}
          >
            {entry.kind === "folder" ? <Folder size={16} /> : <FileText size={16} />}
            <strong>{entry.name}</strong>
            <small>{entry.kind === "folder" ? t("Folder") : formatSize(entry.size)}</small>
            <span>{entry.modified ? new Date(entry.modified).toLocaleString() : "-"}</span>
          </button>
        ))}
        {!filteredEntries.length && <div className="empty-row">{t("No files found.")}</div>}
      </div>
    </div>
  );
}

function paneState(folder: string): PaneState {
  return { folder, entries: [], selected: null, filter: "" };
}

function parentFolder(path: string) {
  return path.replace(/[\\/][^\\/]+$/, "") || path;
}

function joinPath(folder: string, name: string) {
  return `${folder.replace(/[\\/]+$/, "")}\\${name}`;
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

function stripExtension(name: string) {
  return name.replace(/\.[^.]+$/, "");
}

function isInside(path: string, root: string) {
  return path.toLowerCase().startsWith(root.toLowerCase());
}

function countMatches(content: string, query: string, regexp: boolean, caseSensitive: boolean) {
  if (!query) return 0;
  try {
    if (regexp) {
      const flags = `g${caseSensitive ? "" : "i"}`;
      return Array.from(content.matchAll(new RegExp(query, flags))).length;
    }
    const haystack = caseSensitive ? content : content.toLowerCase();
    const needle = caseSensitive ? query : query.toLowerCase();
    return haystack.split(needle).length - 1;
  } catch {
    return 0;
  }
}

function replaceInText(content: string, find: string, replacement: string, regexp: boolean, caseSensitive: boolean, all: boolean) {
  if (!find) return content;
  try {
    if (regexp) {
      return content.replace(new RegExp(find, `${all ? "g" : ""}${caseSensitive ? "" : "i"}`), replacement);
    }
    if (all) {
      return content.split(find).join(replacement);
    }
    return content.replace(find, replacement);
  } catch {
    return content;
  }
}

function languageExtension(languageOrPath: string) {
  const value = languageOrPath.toLowerCase();
  if (value.includes("php") || value.endsWith(".php") || value.endsWith(".phtml")) return php();
  if (value.includes("typescript") || value.endsWith(".ts") || value.endsWith(".tsx")) return javascript({ typescript: true, jsx: value.endsWith(".tsx") });
  if (value.includes("javascript") || value.includes("react") || value.endsWith(".js") || value.endsWith(".jsx")) return javascript({ jsx: true });
  if (value.includes("html") || value.endsWith(".html") || value.endsWith(".htm")) return html();
  if (value.includes("css") || value.endsWith(".css") || value.endsWith(".scss")) return css();
  if (value.includes("json") || value.endsWith(".json")) return json();
  if (value.includes("markdown") || value.endsWith(".md")) return markdown();
  if (value.includes("sql") || value.endsWith(".sql")) return sql();
  return [];
}

const editorTheme = EditorView.theme({
  "&": {
    border: "1px solid var(--line)",
    borderRadius: "6px",
    background: "var(--input)",
    color: "var(--text)"
  },
  ".cm-content": {
    fontFamily: "Consolas, 'Cascadia Mono', monospace",
    fontSize: "13px"
  },
  ".cm-gutters": {
    background: "var(--panel)",
    color: "var(--muted)",
    borderRight: "1px solid var(--line)"
  },
  ".cm-activeLine, .cm-activeLineGutter": {
    background: "color-mix(in srgb, var(--blue) 10%, transparent)"
  }
});

function formatSize(size: number) {
  if (size > 1024 * 1024) return `${(size / 1024 / 1024).toFixed(1)} MB`;
  if (size > 1024) return `${Math.round(size / 1024)} KB`;
  return `${size} B`;
}
