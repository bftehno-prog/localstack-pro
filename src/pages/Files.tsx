import {
  Copy,
  ExternalLink,
  FilePlus,
  FileText,
  Folder,
  FolderOpen,
  FolderPlus,
  RefreshCw,
  RotateCcw,
  Save,
  Search,
  Trash2
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { api } from "../ui/api";
import { useT } from "../ui/i18n";
import type { AppRun, AppSnapshot, ConfigFile, FileEntry } from "../ui/types";

export function FilesPage({ state, run }: { state: AppSnapshot; run: AppRun }) {
  const t = useT();
  const roots = useMemo(() => [
    { label: "Projects", path: state.settings.projectsFolder },
    { label: "App Data", path: state.appDataDir },
    { label: "Services", path: state.settings.servicesFolder },
    { label: "Backups", path: state.settings.backupsFolder },
    ...state.hosts.slice(0, 8).map((host) => ({ label: host.domain, path: host.rootFolder }))
  ], [state.appDataDir, state.hosts, state.settings.backupsFolder, state.settings.projectsFolder, state.settings.servicesFolder]);

  const [folder, setFolder] = useState(roots[0]?.path ?? state.appDataDir);
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [selected, setSelected] = useState<FileEntry | null>(null);
  const [openedFile, setOpenedFile] = useState<ConfigFile | null>(null);
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [nameInput, setNameInput] = useState("");
  const [filter, setFilter] = useState("");
  const [editorSearch, setEditorSearch] = useState("");
  const [wrapLines, setWrapLines] = useState(true);
  const [editorMessage, setEditorMessage] = useState("");
  const [cursor, setCursor] = useState(0);

  const dirty = openedFile ? content !== savedContent : false;
  const selectedPath = selected?.path ?? openedFile?.path ?? folder;
  const filteredEntries = useMemo(() => {
    const query = filter.trim().toLowerCase();
    if (!query) return entries;
    return entries.filter((entry) => entry.name.toLowerCase().includes(query));
  }, [entries, filter]);
  const editorMatches = useMemo(() => {
    const query = editorSearch.trim().toLowerCase();
    if (!query || !content) return 0;
    return content.toLowerCase().split(query).length - 1;
  }, [content, editorSearch]);
  const position = useMemo(() => cursorPosition(content, cursor), [content, cursor]);

  const refresh = async (target = folder) => {
    const result = await run(() => api.listFiles(target), { label: `Reading ${target}...` });
    if (Array.isArray(result)) {
      setFolder(target);
      setEntries(result as FileEntry[]);
    }
  };

  const openEntry = async (entry: FileEntry) => {
    if (dirty && entry.path !== openedFile?.path) {
      setEditorMessage("Save or reload the current file before opening another file.");
      return;
    }
    setSelected(entry);
    setEditorMessage("");
    if (entry.kind === "folder") {
      await refresh(entry.path);
      setOpenedFile(null);
      setContent("");
      setSavedContent("");
      return;
    }
    const result = await run(() => api.readFile(entry.path), { label: `Opening ${entry.name}...` });
    if (result && typeof result === "object" && "content" in result) {
      const file = result as ConfigFile;
      setOpenedFile(file);
      setContent(file.content);
      setSavedContent(file.content);
      setCursor(0);
    }
  };

  const createFolder = async () => {
    const name = nameInput.trim();
    if (!name) return;
    await run(() => api.createFolder(joinPath(folder, name)), { label: `Creating ${name}...` });
    setNameInput("");
    await refresh();
  };

  const createFile = async () => {
    const name = nameInput.trim();
    if (!name) return;
    const path = joinPath(folder, name);
    await run(() => api.createFile(path), { label: `Creating ${name}...` });
    setNameInput("");
    await refresh();
    const entry: FileEntry = { name, path, kind: "file", size: 0 };
    await openEntry(entry);
  };

  const renameSelected = async () => {
    const name = nameInput.trim();
    if (!selected || !name) return;
    const result = await run(() => api.renamePath(selected.path, name), { label: `Renaming ${selected.name}...` });
    setNameInput("");
    if (typeof result === "string" && openedFile?.path === selected.path) {
      setOpenedFile({ ...openedFile, path: result });
    }
    setSelected(null);
    await refresh();
  };

  const duplicateSelected = async () => {
    if (!selected) return;
    await run(() => api.duplicatePath(selected.path), { label: `Duplicating ${selected.name}...` });
    await refresh();
  };

  const deleteSelected = async () => {
    if (!selected) return;
    await run(() => api.deletePath(selected.path), { label: `Deleting ${selected.name}...` });
    if (openedFile?.path === selected.path || selected.kind === "folder") {
      setOpenedFile(null);
      setContent("");
      setSavedContent("");
    }
    setSelected(null);
    await refresh();
  };

  const reloadFile = async () => {
    if (!openedFile) return;
    const result = await run(() => api.readFile(openedFile.path), { label: `Reloading ${openedFile.path}...` });
    if (result && typeof result === "object" && "content" in result) {
      const file = result as ConfigFile;
      setOpenedFile(file);
      setContent(file.content);
      setSavedContent(file.content);
      setEditorMessage("");
    }
  };

  const save = async () => {
    if (!openedFile || openedFile.readOnly) return;
    await run(() => api.writeFile(openedFile.path, content), { label: `Saving ${openedFile.path}...` });
    setSavedContent(content);
    setEditorMessage("Saved.");
    await refresh();
  };

  const copyPath = (value: string) => {
    void navigator.clipboard?.writeText(value);
    setEditorMessage("Path copied.");
  };

  useEffect(() => {
    void refresh(roots[0]?.path ?? state.appDataDir);
  }, [state.appDataDir]);

  return (
    <div className="page-grid files-page-grid">
      <section>
        <div className="page-title">
          <div><h1>{t("File Manager")}</h1><p>{t("Project files, configs, logs and backups")}</p></div>
          <div className="toolbar">
            <input className="toolbar-input" value={nameInput} onChange={(event) => setNameInput(event.target.value)} placeholder={String(t("Name"))} />
            <Button icon={<RefreshCw size={16} />} onClick={() => void refresh()}>{t("Refresh")}</Button>
            <Button icon={<FolderPlus size={16} />} disabled={!nameInput.trim()} onClick={() => void createFolder()}>{t("New Folder")}</Button>
            <Button icon={<FilePlus size={16} />} disabled={!nameInput.trim()} onClick={() => void createFile()}>{t("New File")}</Button>
          </div>
        </div>

        <Panel title={String(t("Files"))} action={<Button icon={<FolderOpen size={16} />} onClick={() => void run(() => api.openPath(folder), { label: `Opening ${folder}...` })}>{t("Open Folder")}</Button>}>
          <div className="file-pathbar">
            <button onClick={() => void refresh(parentFolder(folder))}>..</button>
            <code>{folder}</code>
            <button title={String(t("Copy Path"))} onClick={() => copyPath(folder)}><Copy size={15} /></button>
          </div>
          <div className="file-filter">
            <Search size={16} />
            <input value={filter} onChange={(event) => setFilter(event.target.value)} placeholder={String(t("Search files..."))} />
            <span>{filteredEntries.length} / {entries.length}</span>
          </div>
          <div className="file-manager">
            <aside className="file-roots">
              {roots.map((root) => (
                <button key={root.label} className={isInside(folder, root.path) ? "active" : ""} onClick={() => void refresh(root.path)} title={root.path}>
                  <Folder size={16} />
                  <span>{root.label}</span>
                </button>
              ))}
            </aside>
            <div className="file-list">
              {filteredEntries.map((entry) => (
                <button key={entry.path} className={`file-row ${selected?.path === entry.path ? "selected" : ""}`} onClick={() => void openEntry(entry)} title={entry.path}>
                  {entry.kind === "folder" ? <Folder size={16} /> : <FileText size={16} />}
                  <strong>{entry.name}</strong>
                  <small>{entry.kind === "folder" ? t("Folder") : formatSize(entry.size)}</small>
                  <span>{entry.modified ? new Date(entry.modified).toLocaleString() : "-"}</span>
                </button>
              ))}
              {!filteredEntries.length && <div className="empty-row">{t("No files found.")}</div>}
            </div>
          </div>
        </Panel>
      </section>

      <aside className="detail-rail files-detail-rail">
        <Panel title={String(t("Selection"))}>
          <div className="kv detail-kv">
            <span>{t("Selected")}</span><strong>{selected?.name ?? "-"}</strong>
            <span>{t("Path")}</span><code>{selectedPath}</code>
            {openedFile && <><span>{t("Type")}</span><strong>{openedFile.language ?? "Text"}</strong></>}
            {openedFile && <><span>{t("Size")}</span><strong>{formatSize(openedFile.size ?? content.length)}</strong></>}
            {openedFile && <><span>{t("Modified")}</span><strong>{openedFile.modified ? new Date(openedFile.modified).toLocaleString() : "-"}</strong></>}
            {openedFile && <><span>{t("Status")}</span><strong>{openedFile.readOnly ? t("Read only") : dirty ? t("Unsaved") : t("Saved")}</strong></>}
          </div>
          <div className="quick-grid">
            <Button disabled={!selected || !nameInput.trim()} onClick={() => void renameSelected()}>{t("Rename")}</Button>
            <Button disabled={!selected} onClick={() => void duplicateSelected()}>{t("Duplicate")}</Button>
            <Button disabled={!selected} icon={<Copy size={16} />} onClick={() => copyPath(selectedPath)}>{t("Copy Path")}</Button>
            <Button disabled={!selected} icon={<Trash2 size={16} />} variant="danger" onClick={() => void deleteSelected()}>{t("Delete")}</Button>
          </div>
        </Panel>

        <Panel
          title={openedFile ? `${t("Editor")} - ${fileName(openedFile.path)}` : String(t("Editor"))}
          action={openedFile && (
            <div className="toolbar">
              <Button icon={<RotateCcw size={16} />} disabled={!openedFile} onClick={() => void reloadFile()}>{t("Reload")}</Button>
              <Button icon={<ExternalLink size={16} />} disabled={!openedFile} onClick={() => openedFile && void run(() => api.openPath(openedFile.path), { label: `Opening ${openedFile.path}...` })}>{t("Open")}</Button>
              <Button variant="primary" icon={<Save size={16} />} disabled={!dirty || openedFile.readOnly} onClick={() => void save()}>{t("Save")}</Button>
            </div>
          )}
        >
          {openedFile ? (
            <div className="file-editor-shell">
              <div className="file-editor-toolbar">
                <label><input type="checkbox" checked={wrapLines} onChange={(event) => setWrapLines(event.target.checked)} /> {t("Wrap")}</label>
                <div className="file-filter compact">
                  <Search size={15} />
                  <input value={editorSearch} onChange={(event) => setEditorSearch(event.target.value)} placeholder={String(t("Search in file..."))} />
                  <span>{editorMatches}</span>
                </div>
                <span>{t("Line")} {position.line}, {t("Column")} {position.column}</span>
              </div>
              <textarea
                className={`config-editor file-editor ${wrapLines ? "wrap" : "nowrap"}`}
                value={content}
                readOnly={openedFile.readOnly}
                spellCheck={false}
                onChange={(event) => {
                  setContent(event.target.value);
                  setCursor(event.target.selectionStart);
                  setEditorMessage("");
                }}
                onClick={(event) => setCursor(event.currentTarget.selectionStart)}
                onKeyUp={(event) => setCursor(event.currentTarget.selectionStart)}
              />
              <div className="file-editor-status">
                <span>{dirty ? t("Unsaved changes") : t("No unsaved changes")}</span>
                {editorMessage && <strong>{t(editorMessage)}</strong>}
              </div>
            </div>
          ) : (
            <div className="empty-row">{t("Select a text file to edit.")}</div>
          )}
        </Panel>
      </aside>
    </div>
  );
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

function isInside(path: string, root: string) {
  return path.toLowerCase().startsWith(root.toLowerCase());
}

function cursorPosition(content: string, cursor: number) {
  const before = content.slice(0, cursor);
  const lines = before.split(/\r\n|\r|\n/);
  return { line: lines.length, column: lines[lines.length - 1].length + 1 };
}

function formatSize(size: number) {
  if (size > 1024 * 1024) return `${(size / 1024 / 1024).toFixed(1)} MB`;
  if (size > 1024) return `${Math.round(size / 1024)} KB`;
  return `${size} B`;
}
