import { FileText, Folder, FolderPlus, RefreshCw, Save, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { api } from "../ui/api";
import type { AppRun, AppSnapshot, FileEntry } from "../ui/types";

export function FilesPage({ state, run }: { state: AppSnapshot; run: AppRun }) {
  const roots = useMemo(() => [
    { label: "Projects", path: state.settings.projectsFolder },
    { label: "App Data", path: state.appDataDir },
    { label: "Services", path: state.settings.servicesFolder },
    { label: "Backups", path: state.settings.backupsFolder },
    ...state.hosts.slice(0, 6).map((host) => ({ label: host.domain, path: host.rootFolder }))
  ], [state]);
  const [folder, setFolder] = useState(roots[0]?.path ?? state.appDataDir);
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [selected, setSelected] = useState<FileEntry | null>(null);
  const [editorPath, setEditorPath] = useState("");
  const [content, setContent] = useState("");

  const refresh = async (target = folder) => {
    const result = await run(() => api.listFiles(target), { label: `Reading ${target}...` });
    if (Array.isArray(result)) {
      setFolder(target);
      setEntries(result as FileEntry[]);
    }
  };
  const openEntry = async (entry: FileEntry) => {
    setSelected(entry);
    if (entry.kind === "folder") {
      await refresh(entry.path);
      setEditorPath("");
      setContent("");
      return;
    }
    const result = await run(() => api.readFile(entry.path), { label: `Opening ${entry.name}...` });
    if (result && typeof result === "object" && "content" in result) {
      setEditorPath(result.path);
      setContent(result.content);
    }
  };
  const createFolder = async () => {
    const name = window.prompt("Folder name");
    if (!name?.trim()) return;
    await run(() => api.createFolder(`${folder}\\${name.trim()}`), { label: `Creating ${name}...` });
    await refresh();
  };
  const createFile = async () => {
    const name = window.prompt("File name");
    if (!name?.trim()) return;
    await run(() => api.writeFile(`${folder}\\${name.trim()}`, ""), { label: `Creating ${name}...` });
    await refresh();
  };
  const renameSelected = async () => {
    if (!selected) return;
    const name = window.prompt("New name", selected.name);
    if (!name?.trim()) return;
    await run(() => api.renamePath(selected.path, name.trim()), { label: `Renaming ${selected.name}...` });
    setSelected(null);
    await refresh();
  };
  const deleteSelected = async () => {
    if (!selected || !window.confirm(`Delete ${selected.name}?`)) return;
    await run(() => api.deletePath(selected.path), { label: `Deleting ${selected.name}...` });
    setSelected(null);
    setEditorPath("");
    setContent("");
    await refresh();
  };
  const save = async () => {
    if (!editorPath) return;
    await run(() => api.writeFile(editorPath, content), { label: `Saving ${editorPath}...` });
  };

  useEffect(() => {
    void refresh(roots[0]?.path ?? state.appDataDir);
  }, [state.appDataDir]);

  return (
    <div className="page-grid">
      <section>
        <div className="page-title">
          <div><h1>File Manager</h1><p>Project files, configs, logs and backups</p></div>
          <div className="toolbar">
            <Button icon={<RefreshCw size={16} />} onClick={() => void refresh()}>Refresh</Button>
            <Button icon={<FolderPlus size={16} />} onClick={() => void createFolder()}>New Folder</Button>
            <Button icon={<FileText size={16} />} onClick={() => void createFile()}>New File</Button>
          </div>
        </div>
        <Panel title="Files" action={<Button icon={<Folder size={16} />} onClick={() => void run(() => api.openPath(folder), { label: `Opening ${folder}...` })}>Open Folder</Button>}>
          <div className="file-manager">
            <aside className="file-roots">
              {roots.map((root) => (
                <button key={root.label} className={folder.startsWith(root.path) ? "active" : ""} onClick={() => void refresh(root.path)}>
                  <Folder size={16} />
                  <span>{root.label}</span>
                </button>
              ))}
            </aside>
            <div className="file-list">
              <button className="file-row muted" onClick={() => void refresh(parentFolder(folder))}>..</button>
              {entries.map((entry) => (
                <button key={entry.path} className={`file-row ${selected?.path === entry.path ? "selected" : ""}`} onClick={() => void openEntry(entry)}>
                  {entry.kind === "folder" ? <Folder size={16} /> : <FileText size={16} />}
                  <strong>{entry.name}</strong>
                  <small>{entry.kind === "folder" ? "Folder" : formatSize(entry.size)}</small>
                  <span>{entry.modified ? new Date(entry.modified).toLocaleString() : "-"}</span>
                </button>
              ))}
            </div>
          </div>
        </Panel>
      </section>
      <aside className="detail-rail">
        <Panel title="Selection">
          <div className="kv detail-kv">
            <span>Folder</span><strong>{folder}</strong>
            <span>Selected</span><strong>{selected?.name ?? "-"}</strong>
            <span>Path</span><code>{selected?.path ?? (editorPath || "-")}</code>
          </div>
          <div className="quick-grid">
            <Button disabled={!selected} onClick={() => void renameSelected()}>Rename</Button>
            <Button disabled={!selected} icon={<Trash2 size={16} />} variant="danger" onClick={() => void deleteSelected()}>Delete</Button>
          </div>
        </Panel>
        {editorPath && (
          <Panel title="Editor" action={<Button icon={<Save size={16} />} onClick={() => void save()}>Save</Button>}>
            <textarea className="config-editor file-editor" value={content} onChange={(event) => setContent(event.target.value)} />
          </Panel>
        )}
      </aside>
    </div>
  );
}

function parentFolder(path: string) {
  return path.replace(/[\\/][^\\/]+$/, "") || path;
}

function formatSize(size: number) {
  if (size > 1024 * 1024) return `${(size / 1024 / 1024).toFixed(1)} MB`;
  if (size > 1024) return `${Math.round(size / 1024)} KB`;
  return `${size} B`;
}
