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
  ArrowLeft,
  ArrowRight,
  CheckSquare,
  Copy,
  Edit,
  ExternalLink,
  FilePlus,
  FileText,
  Folder,
  FolderOpen,
  FolderPlus,
  GitCompare,
  MoveRight,
  RefreshCw,
  Replace,
  RotateCcw,
  Save,
  Search,
  Shield,
  Trash2,
  Undo2,
  Upload
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { api } from "../ui/api";
import { useT } from "../ui/i18n";
import type { AppRun, AppSnapshot, ArchiveEntry, ConfigFile, FileEntry, FileSearchResult, TrashRecord } from "../ui/types";

type PaneKey = "left" | "right";
type PaneState = {
  folder: string;
  entries: FileEntry[];
  selected: FileEntry | null;
  filter: string;
  sortKey: "name" | "size" | "modified";
  sortDir: "asc" | "desc";
};

type FileOperation = {
  id: string;
  label: string;
  status: "running" | "done" | "error" | "cancelled";
  message?: string;
  completed?: number;
  total?: number;
};

type FileContextMenu = {
  x: number;
  y: number;
  pane: PaneKey;
  entry: FileEntry;
};

type EditorTab = ConfigFile & {
  draftContent: string;
  savedContent: string;
};

type UndoAction = {
  label: string;
  restoreItems?: TrashRecord[];
  moveBack?: Array<{ from: string; to: string }>;
  removeCreated?: string[];
};

type PaneHistory = Record<PaneKey, { back: string[]; forward: string[] }>;

type SavedSearch = {
  name: string;
  query: string;
  regexp: boolean;
  caseSensitive: boolean;
  includeExtensions: string;
  excludeFolders: string;
};

type FolderCompareRow = {
  name: string;
  status: "left-only" | "right-only" | "changed";
  left?: FileEntry;
  right?: FileEntry;
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
  const [editorTabs, setEditorTabs] = useState<EditorTab[]>([]);
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
  const [favorites, setFavorites] = useState<string[]>(() => loadStringList("localstack.fileFavorites"));
  const [recentFiles, setRecentFiles] = useState<string[]>(() => loadStringList("localstack.recentFiles"));
  const [archiveEntries, setArchiveEntries] = useState<ArchiveEntry[]>([]);
  const [savedSearches, setSavedSearches] = useState<SavedSearch[]>(() => loadJsonList<SavedSearch>("localstack.fileSearches"));
  const [trashItems, setTrashItems] = useState<TrashRecord[]>(() => loadJsonList<TrashRecord>("localstack.fileTrash"));
  const [bulkFind, setBulkFind] = useState("");
  const [bulkReplace, setBulkReplace] = useState("");
  const [bulkRegexp, setBulkRegexp] = useState(false);
  const [compareRows, setCompareRows] = useState<FolderCompareRow[]>([]);
  const [includeExtensions, setIncludeExtensions] = useState("");
  const [excludeFolders, setExcludeFolders] = useState("node_modules,.git,.next,vendor,target");
  const [aclIdentity, setAclIdentity] = useState("Users");
  const [aclRights, setAclRights] = useState("M");
  const [aclInherit, setAclInherit] = useState(true);
  const [showReplace, setShowReplace] = useState(false);
  const [paneHistory, setPaneHistory] = useState<PaneHistory>({ left: { back: [], forward: [] }, right: { back: [], forward: [] } });
  const [lastUndo, setLastUndo] = useState<UndoAction | null>(null);
  const [diffText, setDiffText] = useState("");
  const [autosave, setAutosave] = useState(false);
  const [autosaveSeconds, setAutosaveSeconds] = useState(20);
  const [jumpLine, setJumpLine] = useState("");
  const cancelledOperations = useRef(new Set<string>());
  const folderCache = useRef(new Map<string, { time: number; entries: FileEntry[] }>());

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
  const paneRoots = useMemo(() => [
    ...favorites.map((path) => ({ label: `* ${fileName(path) || path}`, path })),
    ...roots
  ], [favorites, roots]);
  const selectedFilesLabel = selectedEntries.length > 1 ? `${selectedEntries.length} files` : selectedEntries[0]?.name ?? "";
  const editorMatches = useMemo(() => countMatches(content, editorSearch, editorRegexp, caseSensitive), [caseSensitive, content, editorRegexp, editorSearch]);
  const jsonError = useMemo(() => validateJson(content, openedFile?.language ?? openedFile?.path ?? ""), [content, openedFile]);
  const codeDiagnostics = useMemo(() => quickCodeDiagnostics(content, openedFile?.language ?? openedFile?.path ?? ""), [content, openedFile]);

  const setPane = (key: PaneKey, next: PaneState | ((current: PaneState) => PaneState)) => {
    if (key === "left") setLeftPane(next);
    else setRightPane(next);
  };

  const setPaneSelectedPaths = (key: PaneKey, paths: string[]) => {
    setSelectedPaths((current) => ({ ...current, [key]: paths }));
  };

  const runFileOperation = async (label: string, total: number, action: (helpers: { isCancelled: () => boolean; progress: (completed: number) => void }) => Promise<void>) => {
    const id = crypto.randomUUID();
    const operation: FileOperation = { id, label, status: "running", completed: 0, total };
    setOperationQueue((current) => [operation, ...current].slice(0, 8));
    try {
      folderCache.current.clear();
      await action({
        isCancelled: () => cancelledOperations.current.has(id),
        progress: (completed) => setOperationQueue((current) => current.map((item) => item.id === id ? { ...item, completed } : item))
      });
      setOperationQueue((current) => current.map((item) => item.id === id ? { ...item, status: cancelledOperations.current.has(id) ? "cancelled" : "done" } : item));
    } catch (error) {
      setOperationQueue((current) => current.map((item) => item.id === id ? { ...item, status: "error", message: String(error) } : item));
    } finally {
      cancelledOperations.current.delete(id);
    }
  };

  const cancelOperation = (id: string) => {
    cancelledOperations.current.add(id);
    setOperationQueue((current) => current.map((item) => item.id === id ? { ...item, status: "cancelled" } : item));
  };

  const syncCurrentTab = (nextContent = content, nextSavedContent = savedContent, file = openedFile) => {
    if (!file) return;
    setEditorTabs((current) => current.map((tab) => tab.path === file.path ? { ...tab, ...file, draftContent: nextContent, savedContent: nextSavedContent, encoding } : tab));
  };

  const refresh = async (key: PaneKey = activePane, target?: string, pushHistory = true) => {
    const current = key === "left" ? leftPane : rightPane;
    const folder = target ?? current.folder;
    const cached = folderCache.current.get(folder);
    if (cached && Date.now() - cached.time < 5000) {
      setPane(key, { ...current, folder, entries: cached.entries, selected: null });
      setPaneSelectedPaths(key, []);
      return;
    }
    const result = await run(() => api.listFiles(folder), { label: `Reading ${folder}...` });
    if (Array.isArray(result)) {
      folderCache.current.set(folder, { time: Date.now(), entries: result as FileEntry[] });
      if (pushHistory && folder !== current.folder) {
        setPaneHistory((history) => ({
          ...history,
          [key]: { back: [...history[key].back, current.folder].slice(-40), forward: [] }
        }));
      }
      setPane(key, { ...current, folder, entries: result as FileEntry[], selected: null });
      setPaneSelectedPaths(key, []);
    }
  };

  const navigateHistory = async (key: PaneKey, direction: "back" | "forward") => {
    const current = key === "left" ? leftPane : rightPane;
    const history = paneHistory[key];
    const stack = direction === "back" ? history.back : history.forward;
    const target = stack[stack.length - 1];
    if (!target) return;
    setPaneHistory((value) => ({
      ...value,
      [key]: direction === "back"
        ? { back: value[key].back.slice(0, -1), forward: [current.folder, ...value[key].forward].slice(0, 40) }
        : { back: [...value[key].back, current.folder].slice(-40), forward: value[key].forward.slice(1) }
    }));
    await refresh(key, target, false);
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
      await refresh(key, entry.path);
      return;
    }
    await openFile(entry.path, encoding, true);
  };

  const openFile = async (path: string, fileEncoding = encoding, showModal = false) => {
    syncCurrentTab();
    const existing = editorTabs.find((tab) => tab.path === path && tab.encoding === fileEncoding);
    if (existing) {
      setOpenedFile(existing);
      setContent(existing.draftContent);
      setSavedContent(existing.savedContent);
      setEncoding(existing.encoding ?? fileEncoding);
      rememberRecentFile(path);
      if (showModal) setEditorOpen(true);
      return;
    }
    const result = await run(() => api.readFileWithEncoding(path, fileEncoding), { label: `Opening ${fileName(path)}...` });
    if (result && typeof result === "object" && "content" in result) {
      const file = result as ConfigFile;
      setOpenedFile(file);
      setContent(file.content);
      setSavedContent(file.content);
      setEncoding(file.encoding ?? fileEncoding);
      setEditorTabs((current) => [{ ...file, draftContent: file.content, savedContent: file.content }, ...current.filter((tab) => tab.path !== file.path)].slice(0, 8));
      rememberRecentFile(path);
      if (showModal) setEditorOpen(true);
    }
  };

  const activateEditorTab = (path: string) => {
    syncCurrentTab();
    const tab = editorTabs.find((item) => item.path === path);
    if (!tab) return;
    setOpenedFile(tab);
    setContent(tab.draftContent);
    setSavedContent(tab.savedContent);
    setEncoding(tab.encoding ?? encoding);
    setDiffText("");
  };

  const closeEditorTab = (path: string) => {
    const nextTabs = editorTabs.filter((tab) => tab.path !== path);
    setEditorTabs(nextTabs);
    if (openedFile?.path !== path) return;
    const next = nextTabs[0];
    if (next) {
      setOpenedFile(next);
      setContent(next.draftContent);
      setSavedContent(next.savedContent);
      setEncoding(next.encoding ?? encoding);
    } else {
      closeEditor();
    }
  };

  const createFolder = async () => {
    const name = nameInput.trim();
    if (!name) return;
    await run(() => api.createFolder(joinPath(pane.folder, name)), { label: `Creating ${name}...` });
    folderCache.current.delete(pane.folder);
    setNameInput("");
    await refresh(activePane);
  };

  const createFile = async () => {
    const name = nameInput.trim();
    if (!name) return;
    const path = joinPath(pane.folder, name);
    await run(() => api.createFile(path), { label: `Creating ${name}...` });
    folderCache.current.delete(pane.folder);
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
    folderCache.current.delete(pane.folder);
    await refresh(activePane);
  };

  const copySelected = async (toOtherPane = false) => {
    if (!selectedEntries.length) return;
    const target = targetPath.trim() || (toOtherPane ? otherPane.folder : pane.folder);
    const created: string[] = [];
    await runFileOperation(`Copy ${selectedFilesLabel}`, selectedEntries.length, async ({ isCancelled, progress }) => {
      for (const [index, item] of selectedEntries.entries()) {
        if (isCancelled()) break;
        const result = await run(() => api.copyPath(item.path, target, overwrite), { label: `Copying ${item.name}...`, silent: selectedEntries.length > 1 });
        if (typeof result === "string") created.push(result);
        progress(index + 1);
      }
      if (created.length) setLastUndo({ label: `Undo copy ${selectedFilesLabel}`, removeCreated: created });
      await refresh(activePane);
      await refresh(activePane === "left" ? "right" : "left");
    });
  };

  const moveSelected = async (toOtherPane = false) => {
    if (!selectedEntries.length) return;
    const target = targetPath.trim() || (toOtherPane ? otherPane.folder : pane.folder);
    const moved: Array<{ from: string; to: string }> = [];
    await runFileOperation(`Move ${selectedFilesLabel}`, selectedEntries.length, async ({ isCancelled, progress }) => {
      for (const [index, item] of selectedEntries.entries()) {
        if (isCancelled()) break;
        const result = await run(() => api.movePath(item.path, target, overwrite), { label: `Moving ${item.name}...`, silent: selectedEntries.length > 1 });
        if (typeof result === "string") moved.push({ from: result, to: item.path });
        progress(index + 1);
      }
      if (moved.length) setLastUndo({ label: `Undo move ${selectedFilesLabel}`, moveBack: moved.reverse() });
      if (openedFile && selectedEntries.some((item) => item.path === openedFile.path)) setOpenedFile(null);
      await refresh(activePane);
      await refresh(activePane === "left" ? "right" : "left");
    });
  };

  const deleteSelected = async () => {
    if (!selectedEntries.length) return;
    const trashed: TrashRecord[] = [];
    await runFileOperation(`Delete ${selectedFilesLabel}`, selectedEntries.length, async ({ isCancelled, progress }) => {
      for (const [index, item] of selectedEntries.entries()) {
        if (isCancelled()) break;
        const result = await run(() => api.trashPath(item.path), { label: `Moving ${item.name} to trash...`, silent: selectedEntries.length > 1 });
        if (result && typeof result === "object" && "trashPath" in result) trashed.push(result as TrashRecord);
        progress(index + 1);
      }
      if (trashed.length) {
        const nextTrash = [...trashed, ...trashItems].slice(0, 100);
        setTrashItems(nextTrash);
        localStorage.setItem("localstack.fileTrash", JSON.stringify(nextTrash));
        setLastUndo({ label: `Undo delete ${selectedFilesLabel}`, restoreItems: trashed.reverse() });
      }
      if (openedFile && selectedEntries.some((item) => item.path === openedFile.path || item.kind === "folder")) closeEditor();
      await refresh(activePane);
    });
  };

  const undoLastOperation = async () => {
    if (!lastUndo) return;
    const action = lastUndo;
    setLastUndo(null);
    const total = (action.restoreItems?.length ?? 0) + (action.moveBack?.length ?? 0) + (action.removeCreated?.length ?? 0);
    await runFileOperation(action.label, Math.max(1, total), async ({ progress }) => {
      let completed = 0;
      for (const item of action.restoreItems ?? []) {
        await run(() => api.restoreTrashPath(item.originalPath, item.trashPath, overwrite), { label: `Restoring ${item.name}...`, silent: true });
        setTrashItems((current) => {
          const next = current.filter((entry) => entry.trashPath !== item.trashPath);
          localStorage.setItem("localstack.fileTrash", JSON.stringify(next));
          return next;
        });
        progress(++completed);
      }
      for (const item of action.moveBack ?? []) {
        await run(() => api.movePath(item.from, item.to, true), { label: `Restoring ${fileName(item.to)}...`, silent: true });
        progress(++completed);
      }
      for (const path of action.removeCreated ?? []) {
        await run(() => api.trashPath(path), { label: `Removing ${fileName(path)}...`, silent: true });
        progress(++completed);
      }
      await refresh("left");
      await refresh("right");
    });
  };

  const restoreTrashItem = async (item: TrashRecord) => {
    await run(() => api.restoreTrashPath(item.originalPath, item.trashPath, overwrite), { label: `Restoring ${item.name}...` });
    const next = trashItems.filter((entry) => entry.trashPath !== item.trashPath);
    setTrashItems(next);
    localStorage.setItem("localstack.fileTrash", JSON.stringify(next));
    await refresh("left");
    await refresh("right");
  };

  const emptyTrash = async () => {
    if (!trashItems.length) return;
    await runFileOperation(`Empty trash`, trashItems.length, async ({ progress }) => {
      for (const [index, item] of trashItems.entries()) {
        await run(() => api.deletePath(item.trashPath), { label: `Deleting ${item.name}...`, silent: true }).catch(() => undefined);
        progress(index + 1);
      }
      setTrashItems([]);
      localStorage.setItem("localstack.fileTrash", "[]");
    });
  };

  const bulkRenameSelected = async () => {
    if (!selectedEntries.length || !bulkFind) return;
    const moved: Array<{ from: string; to: string }> = [];
    await runFileOperation(`Bulk rename ${selectedEntries.length} item(s)`, selectedEntries.length, async ({ isCancelled, progress }) => {
      for (const [index, item] of selectedEntries.entries()) {
        if (isCancelled()) break;
        const nextName = renameWithPattern(item.name, bulkFind, bulkReplace, bulkRegexp);
        if (nextName && nextName !== item.name) {
          const result = await run(() => api.renamePath(item.path, nextName), { label: `Renaming ${item.name}...`, silent: selectedEntries.length > 1 });
          if (typeof result === "string") moved.push({ from: result, to: item.path });
        }
        progress(index + 1);
      }
      if (moved.length) setLastUndo({ label: "Undo bulk rename", moveBack: moved.reverse() });
      await refresh(activePane);
    });
  };

  const compareFolders = () => {
    const rightByName = new Map(rightPane.entries.map((entry) => [entry.name.toLowerCase(), entry]));
    const leftByName = new Map(leftPane.entries.map((entry) => [entry.name.toLowerCase(), entry]));
    const rows: FolderCompareRow[] = [];
    for (const left of leftPane.entries) {
      const right = rightByName.get(left.name.toLowerCase());
      if (!right) rows.push({ name: left.name, status: "left-only", left });
      else if (left.kind !== right.kind || left.size !== right.size) rows.push({ name: left.name, status: "changed", left, right });
    }
    for (const right of rightPane.entries) {
      if (!leftByName.has(right.name.toLowerCase())) rows.push({ name: right.name, status: "right-only", right });
    }
    setCompareRows(rows.slice(0, 200));
  };

  const moveDroppedPaths = async (key: PaneKey, paths: string[], mode: "copy" | "move") => {
    if (!paths.length) return;
    const destination = key === "left" ? leftPane.folder : rightPane.folder;
    const sourceEntries = paths.map((path) => ({ path, name: fileName(path) }));
    await runFileOperation(`${mode === "copy" ? "Copy" : "Move"} ${paths.length} item(s)`, paths.length, async ({ isCancelled, progress }) => {
      for (const [index, item] of sourceEntries.entries()) {
        if (isCancelled()) break;
        if (mode === "copy") await run(() => api.copyPath(item.path, destination, overwrite), { label: `Copying ${item.name}...`, silent: true });
        else await run(() => api.movePath(item.path, destination, overwrite), { label: `Moving ${item.name}...`, silent: true });
        progress(index + 1);
      }
      await refresh("left");
      await refresh("right");
    });
  };

  const chmodSelected = async () => {
    if (!selectedEntries.length) return;
    const readOnly = ["400", "440", "444", "500", "550", "555"].includes(chmodMode.trim());
    await runFileOperation(`chmod ${selectedFilesLabel}`, selectedEntries.length, async ({ isCancelled, progress }) => {
      for (const [index, item] of selectedEntries.entries()) {
        if (isCancelled()) break;
        await run(() => api.chmodPath(item.path, chmodMode, readOnly), { label: `Changing permissions for ${item.name}...`, silent: selectedEntries.length > 1 });
        progress(index + 1);
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
    await runFileOperation(`Upload ${sources.length} file(s)`, sources.length, async ({ progress }) => {
      await run(() => api.uploadFiles(sources, destination, overwrite), { label: `Uploading ${sources.length} file(s)...` });
      progress(sources.length);
      await refresh(key);
    });
  };

  const extractSelected = async () => {
    if (!selectedEntries.length) return;
    const archives = selectedEntries.filter((item) => item.kind === "file");
    await runFileOperation(`Extract ${archives.length} archive(s)`, archives.length, async ({ isCancelled, progress }) => {
      for (const [index, item] of archives.entries()) {
        if (isCancelled()) break;
        await run(() => api.extractArchiveTo(item.path, targetPath.trim() || pane.folder), { label: `Extracting ${item.name}...`, silent: archives.length > 1 });
        progress(index + 1);
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
    await runFileOperation(`Archive ${selectedFilesLabel}`, selectedEntries.length, async ({ progress }) => {
      await run(() => api.createArchive(paths, target), { label: `Creating archive ${fileName(target)}...` });
      progress(selectedEntries.length);
      setArchivePath(target);
      await refresh(activePane);
    });
  };

  const saveFile = async () => {
    if (!openedFile || openedFile.readOnly) return;
    await run(() => api.writeFileWithEncoding(openedFile.path, content, encoding), { label: `Saving ${openedFile.path}...` });
    setSavedContent(content);
    setEditorTabs((current) => current.map((tab) => tab.path === openedFile.path ? { ...tab, draftContent: content, savedContent: content, encoding } : tab));
    setEditorMessage("Saved.");
    await refresh("left");
    await refresh("right");
  };

  const saveFileAs = async () => {
    if (!openedFile) return;
    const target = await saveDialog({ defaultPath: openedFile.path });
    if (!target) return;
    await run(() => api.writeFileWithEncoding(target, content, encoding), { label: `Saving ${target}...` });
    await openFile(target, encoding, true);
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
    syncCurrentTab(next);
  };

  const replaceAll = () => {
    const next = replaceInText(content, editorSearch, editorReplace, editorRegexp, caseSensitive, true);
    setContent(next);
    syncCurrentTab(next);
  };

  const formatCurrentFile = () => {
    const next = formatEditorContent(content, openedFile?.language ?? openedFile?.path ?? "");
    setContent(next);
    syncCurrentTab(next);
    setEditorMessage(next === content ? "Nothing to format." : "Formatted.");
  };

  const compareTabs = () => {
    if (!openedFile || editorTabs.length < 2) return;
    syncCurrentTab();
    const other = editorTabs.find((tab) => tab.path !== openedFile.path);
    if (!other) return;
    setDiffText(buildLineDiff(fileName(openedFile.path), content, fileName(other.path), other.draftContent));
  };

  const jumpToLine = () => {
    const line = Math.max(1, Number.parseInt(jumpLine, 10) || 1);
    const target = document.querySelector(`.cm-content .cm-line:nth-child(${line})`);
    target?.scrollIntoView({ block: "center" });
  };

  const searchContents = async () => {
    if (!contentSearch.trim()) {
      setContentResults([]);
      return;
    }
    const result = await run(() => api.searchFileContentsAdvanced(pane.folder, contentSearch, editorRegexp, caseSensitive, includeExtensions, excludeFolders, 1000), { label: `Searching ${pane.folder}...` });
    if (Array.isArray(result)) setContentResults(result as FileSearchResult[]);
  };

  const saveSearchFilter = () => {
    const name = contentSearch.trim() || `Search ${savedSearches.length + 1}`;
    const next = [{ name, query: contentSearch, regexp: editorRegexp, caseSensitive, includeExtensions, excludeFolders }, ...savedSearches.filter((item) => item.name !== name)].slice(0, 8);
    setSavedSearches(next);
    localStorage.setItem("localstack.fileSearches", JSON.stringify(next));
  };

  const applySearchFilter = (name: string) => {
    const preset = savedSearches.find((item) => item.name === name);
    if (!preset) return;
    setContentSearch(preset.query);
    setEditorRegexp(preset.regexp);
    setCaseSensitive(preset.caseSensitive);
    setIncludeExtensions(preset.includeExtensions);
    setExcludeFolders(preset.excludeFolders);
  };

  const copyPath = (value: string) => {
    void navigator.clipboard?.writeText(value);
    setEditorMessage("Path copied.");
  };

  const toggleFavorite = (path = pane.folder) => {
    setFavorites((current) => {
      const next = current.includes(path) ? current.filter((item) => item !== path) : [path, ...current].slice(0, 12);
      localStorage.setItem("localstack.fileFavorites", JSON.stringify(next));
      return next;
    });
  };

  const rememberRecentFile = (path: string) => {
    setRecentFiles((current) => {
      const next = [path, ...current.filter((item) => item !== path)].slice(0, 10);
      localStorage.setItem("localstack.recentFiles", JSON.stringify(next));
      return next;
    });
  };

  const selectAllInPane = (key: PaneKey) => {
    const current = key === "left" ? leftPane : rightPane;
    setPaneSelectedPaths(key, current.entries.map((entry) => entry.path));
  };

  const invertSelectionInPane = (key: PaneKey) => {
    const current = key === "left" ? leftPane : rightPane;
    const selectedSet = new Set(selectedPaths[key]);
    setPaneSelectedPaths(key, current.entries.filter((entry) => !selectedSet.has(entry.path)).map((entry) => entry.path));
  };

  const changePaneSort = (key: PaneKey, sortKey: PaneState["sortKey"]) => {
    setPane(key, (current) => ({ ...current, sortKey, sortDir: current.sortKey === sortKey && current.sortDir === "asc" ? "desc" : "asc" }));
  };

  const previewArchive = async () => {
    const entry = selectedEntries[0];
    if (!entry || entry.kind !== "file") return;
    const result = await run(() => api.listArchiveEntries(entry.path), { label: `Reading archive ${entry.name}...` });
    if (Array.isArray(result)) setArchiveEntries(result as ArchiveEntry[]);
  };

  const applyAcl = async () => {
    if (!selectedEntries.length) return;
    await runFileOperation(`ACL ${selectedFilesLabel}`, selectedEntries.length, async ({ isCancelled, progress }) => {
      for (const [index, item] of selectedEntries.entries()) {
        if (isCancelled()) break;
        await run(() => api.applyWindowsAcl(item.path, aclIdentity, aclRights, aclInherit), { label: `Applying ACL to ${item.name}...`, silent: selectedEntries.length > 1 });
        progress(index + 1);
      }
      await refresh(activePane);
    });
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
    syncCurrentTab();
    setOpenedFile(null);
    setContent("");
    setSavedContent("");
    setEditorTabs([]);
    setEditorOpen(false);
  };

  useEffect(() => {
    void refresh("left", roots[0]?.path ?? state.appDataDir);
    void refresh("right", state.appDataDir);
  }, [state.appDataDir]);

  useEffect(() => {
    try {
      const session = JSON.parse(localStorage.getItem("localstack.unsavedEditor") ?? "null") as { file: ConfigFile; content: string; savedContent: string; encoding: string } | null;
      if (!session?.file?.path || session.content === session.savedContent) return;
      const tab = { ...session.file, draftContent: session.content, savedContent: session.savedContent, encoding: session.encoding };
      setOpenedFile(session.file);
      setEditorTabs([tab]);
      setContent(session.content);
      setSavedContent(session.savedContent);
      setEncoding(session.encoding);
      setEditorOpen(true);
      setEditorMessage("Unsaved session restored.");
    } catch {
      localStorage.removeItem("localstack.unsavedEditor");
    }
  }, []);

  useEffect(() => {
    if (!openedFile || content === savedContent) {
      localStorage.removeItem("localstack.unsavedEditor");
      return;
    }
    localStorage.setItem("localstack.unsavedEditor", JSON.stringify({ file: openedFile, content, savedContent, encoding }));
  }, [content, encoding, openedFile, savedContent]);

  useEffect(() => {
    if (!editorOpen) return;
    const handleKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      const key = event.key.toLowerCase();
      if (key === "s") {
        event.preventDefault();
        void saveFile();
      } else if (key === "f") {
        event.preventDefault();
        setShowReplace(false);
        document.querySelector<HTMLInputElement>("[data-editor-find]")?.focus();
      } else if (key === "h") {
        event.preventDefault();
        setShowReplace(true);
        document.querySelector<HTMLInputElement>("[data-editor-replace]")?.focus();
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [editorOpen, openedFile, content, encoding]);

  useEffect(() => {
    if (!editorOpen || !openedFile) return;
    const timer = window.setInterval(async () => {
      if (dirty) return;
      try {
        const fresh = await api.readFileWithEncoding(openedFile.path, encoding);
        if (fresh.modified && openedFile.modified && fresh.modified !== openedFile.modified) {
          setEditorMessage("File changed on disk.");
        }
      } catch {
        setEditorMessage("File is no longer available.");
      }
    }, 5000);
    return () => window.clearInterval(timer);
  }, [dirty, editorOpen, encoding, openedFile]);

  useEffect(() => {
    if (!autosave || !dirty || !openedFile || openedFile.readOnly) return;
    const timer = window.setTimeout(() => void saveFile(), Math.max(5, autosaveSeconds) * 1000);
    return () => window.clearTimeout(timer);
  }, [autosave, autosaveSeconds, dirty, openedFile, content]);

  return (
    <div className="files-workspace">
      <div className="page-title">
        <div><h1>{t("File Manager")}</h1><p>{t("Project files, configs, logs and backups")}</p></div>
        <div className="toolbar files-main-toolbar">
          <input className="toolbar-input" value={nameInput} onChange={(event) => setNameInput(event.target.value)} placeholder={String(t("File or folder name"))} />
          <Button icon={<FolderPlus size={16} />} disabled={!nameInput.trim()} onClick={() => void createFolder()}>New Folder</Button>
          <Button icon={<FilePlus size={16} />} disabled={!nameInput.trim()} onClick={() => void createFile()}>New File</Button>
          <Button icon={<Upload size={16} />} onClick={() => void uploadIntoPane()}>Upload</Button>
          <Button onClick={() => toggleFavorite()}>{favorites.includes(pane.folder) ? "Unfavorite" : "Favorite"}</Button>
        </div>
      </div>

      <Panel title="Files">
        <div className="dual-file-manager">
          <FilePane paneKey="left" pane={leftPane} active={activePane === "left"} selectedPaths={selectedPaths.left} roots={paneRoots} history={paneHistory.left} onActivate={() => setActivePane("left")} onRefresh={(path) => void refresh("left", path)} onHistory={(direction) => void navigateHistory("left", direction)} onFilter={(filter) => setLeftPane((current) => ({ ...current, filter }))} onSort={changePaneSort} onSelectAll={selectAllInPane} onInvertSelection={invertSelectionInPane} onSelect={selectEntry} onOpen={(entry) => void openEntry("left", entry)} onContextMenu={openContextMenu} onDropFiles={uploadDroppedFiles} onDropPaths={moveDroppedPaths} onCopyPath={copyPath} />
          <FilePane paneKey="right" pane={rightPane} active={activePane === "right"} selectedPaths={selectedPaths.right} roots={paneRoots} history={paneHistory.right} onActivate={() => setActivePane("right")} onRefresh={(path) => void refresh("right", path)} onHistory={(direction) => void navigateHistory("right", direction)} onFilter={(filter) => setRightPane((current) => ({ ...current, filter }))} onSort={changePaneSort} onSelectAll={selectAllInPane} onInvertSelection={invertSelectionInPane} onSelect={selectEntry} onOpen={(entry) => void openEntry("right", entry)} onContextMenu={openContextMenu} onDropFiles={uploadDroppedFiles} onDropPaths={moveDroppedPaths} onCopyPath={copyPath} />
        </div>
      </Panel>

      <div className="file-tools-grid">
        <Panel title="Operations">
          <div className="file-ops-grid">
            <input className="toolbar-input" value={targetPath} onChange={(event) => setTargetPath(event.target.value)} placeholder={String(t("Absolute target path"))} />
            <label className="check-line"><input type="checkbox" checked={overwrite} onChange={(event) => setOverwrite(event.target.checked)} /> {t("Overwrite")}</label>
            <Button disabled={!lastUndo} icon={<Undo2 size={16} />} onClick={() => void undoLastOperation()}>Undo</Button>
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
            <input className="toolbar-input" value={aclIdentity} onChange={(event) => setAclIdentity(event.target.value)} placeholder="Users" />
            <select className="toolbar-input compact-select" value={aclRights} onChange={(event) => setAclRights(event.target.value)}>
              <option value="R">Read</option>
              <option value="RX">Read/Execute</option>
              <option value="M">Modify</option>
              <option value="F">Full</option>
            </select>
            <label className="check-line"><input type="checkbox" checked={aclInherit} onChange={(event) => setAclInherit(event.target.checked)} /> Inherit</label>
            <Button disabled={!selectedEntries.length} icon={<Shield size={16} />} onClick={() => void applyAcl()}>Apply ACL</Button>
            <input className="toolbar-input" value={archivePath} onChange={(event) => setArchivePath(event.target.value)} placeholder={String(t("Archive path"))} />
            <Button disabled={!selectedEntries.length} icon={<Archive size={16} />} onClick={() => void createArchive()}>Create Archive</Button>
            <Button disabled={!selectedEntries.length} icon={<Archive size={16} />} onClick={() => void extractSelected()}>Extract Here</Button>
            <Button disabled={!selectedEntries.length} icon={<Archive size={16} />} onClick={() => void previewArchive()}>Preview Archive</Button>
            <Button icon={<FolderOpen size={16} />} onClick={() => void run(() => api.openPath(pane.folder), { label: `Opening ${pane.folder}...` })}>Open Folder</Button>
          </div>
        </Panel>
      </div>

      <div className="file-tools-grid">
        <Panel title="Bulk Rename and Compare">
          <div className="file-ops-grid compact">
            <input className="toolbar-input" value={bulkFind} onChange={(event) => setBulkFind(event.target.value)} placeholder={String(t("Find in name"))} />
            <input className="toolbar-input" value={bulkReplace} onChange={(event) => setBulkReplace(event.target.value)} placeholder={String(t("Replace with"))} />
            <label className="check-line"><input type="checkbox" checked={bulkRegexp} onChange={(event) => setBulkRegexp(event.target.checked)} /> RegExp</label>
            <Button disabled={!selectedEntries.length || !bulkFind} onClick={() => void bulkRenameSelected()}>Bulk Rename</Button>
            <Button icon={<GitCompare size={16} />} onClick={compareFolders}>Compare Panes</Button>
          </div>
          <div className="content-results compact-results">
            {compareRows.map((row) => (
              <button key={`${row.status}-${row.name}`} title={row.name}>
                <strong>{row.name}</strong>
                <span>{row.status} · {row.left ? formatSize(row.left.size) : "-"} / {row.right ? formatSize(row.right.size) : "-"}</span>
              </button>
            ))}
            {!compareRows.length && <div className="empty-row">{t("No folder comparison yet.")}</div>}
          </div>
        </Panel>

        <Panel title="Trash" action={<Button variant="danger" disabled={!trashItems.length} onClick={() => void emptyTrash()}>Empty</Button>}>
          <div className="content-results compact-results">
            {trashItems.slice(0, 20).map((item) => (
              <button key={item.trashPath} title={item.originalPath}>
                <strong>{item.name}</strong>
                <span>{item.kind} · {item.originalPath}</span>
                <small onClick={(event) => { event.stopPropagation(); void restoreTrashItem(item); }}>{t("Restore")}</small>
              </button>
            ))}
            {!trashItems.length && <div className="empty-row">{t("Trash is empty.")}</div>}
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
                <span>{operation.completed ?? 0}/{operation.total ?? 0} · {t(operation.status)}</span>
                {operation.status === "running" && <button onClick={() => cancelOperation(operation.id)}>{t("Cancel")}</button>}
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
              <input className="toolbar-input" value={includeExtensions} onChange={(event) => setIncludeExtensions(event.target.value)} placeholder=".php,.ts,.json" />
              <input className="toolbar-input" value={excludeFolders} onChange={(event) => setExcludeFolders(event.target.value)} placeholder="node_modules,.git" />
              <select className="toolbar-input compact-select" defaultValue="" onChange={(event) => applySearchFilter(event.target.value)}>
                <option value="">Saved</option>
                {savedSearches.map((item) => <option key={item.name} value={item.name}>{item.name}</option>)}
              </select>
              <Button onClick={saveSearchFilter}>Save Filter</Button>
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
            <div className="content-results">
              {archiveEntries.slice(0, 80).map((entry) => (
                <button key={`${entry.path}:${entry.size}`} title={entry.path}>
                  <strong>{entry.path}</strong>
                  <span>{entry.kind} · {formatSize(entry.size)}</span>
                </button>
              ))}
            </div>
            <div className="content-results">
              {recentFiles.map((path) => (
                <button key={path} onClick={() => void openFile(path, encoding, true)} title={path}>
                  <strong>{fileName(path)}</strong>
                  <span>{path}</span>
                </button>
              ))}
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
                <Button icon={<Save size={16} />} onClick={() => void saveFileAs()}>Save As</Button>
                <Button onClick={() => setEditorOpen(false)}>Close</Button>
              </div>
            </div>
            {editorTabs.length > 0 && (
              <div className="editor-tabs">
                {editorTabs.map((tab) => {
                  const tabDirty = tab.path === openedFile.path ? dirty : tab.draftContent !== tab.savedContent;
                  return (
                    <button key={tab.path} className={tab.path === openedFile.path ? "active" : ""} onClick={() => activateEditorTab(tab.path)} title={tab.path}>
                      <FileText size={14} />
                      <span>{fileName(tab.path)}{tabDirty ? " *" : ""}</span>
                      <i onClick={(event) => { event.stopPropagation(); closeEditorTab(tab.path); }}>x</i>
                    </button>
                  );
                })}
              </div>
            )}
            <div className="editor-findbar">
              <input data-editor-find className="toolbar-input" value={editorSearch} onChange={(event) => setEditorSearch(event.target.value)} placeholder={String(t("Find"))} />
              {showReplace && <input data-editor-replace className="toolbar-input" value={editorReplace} onChange={(event) => setEditorReplace(event.target.value)} placeholder={String(t("Replace"))} />}
              <label className="check-line"><input type="checkbox" checked={editorRegexp} onChange={(event) => setEditorRegexp(event.target.checked)} /> RegExp</label>
              <label className="check-line"><input type="checkbox" checked={caseSensitive} onChange={(event) => setCaseSensitive(event.target.checked)} /> Aa</label>
              <label className="check-line"><input type="checkbox" checked={wrapLines} onChange={(event) => setWrapLines(event.target.checked)} /> {t("Wrap")}</label>
              <label className="check-line"><input type="checkbox" checked={autosave} onChange={(event) => setAutosave(event.target.checked)} /> Autosave</label>
              <input className="toolbar-input line-input" value={autosaveSeconds} type="number" min={5} max={300} onChange={(event) => setAutosaveSeconds(Number(event.target.value) || 20)} />
              <input className="toolbar-input line-input" value={jumpLine} onChange={(event) => setJumpLine(event.target.value)} placeholder={String(t("Line"))} />
              <Button onClick={jumpToLine}>Go</Button>
              <Button onClick={() => setShowReplace((value) => !value)}>{showReplace ? "Hide Replace" : "Show Replace"}</Button>
              <Button icon={<Replace size={16} />} disabled={!editorSearch} onClick={replaceNext}>Replace</Button>
              <Button icon={<Replace size={16} />} disabled={!editorSearch} onClick={replaceAll}>Replace All</Button>
              <Button icon={<GitCompare size={16} />} disabled={editorTabs.length < 2} onClick={compareTabs}>Diff</Button>
              <Button onClick={formatCurrentFile}>Format</Button>
            </div>
            <CodeMirror
              value={content}
              height="calc(100vh - 260px)"
              extensions={[languageExtension(openedFile.language ?? openedFile.path), editorTheme, wrapLines ? EditorView.lineWrapping : []]}
              basicSetup={{ searchKeymap: true, foldGutter: true, highlightActiveLine: true, highlightSelectionMatches: true }}
              editable={!openedFile.readOnly}
              onChange={(value) => {
                setContent(value);
                syncCurrentTab(value);
                setEditorMessage("");
                setDiffText("");
              }}
            />
            {diffText && <pre className="diff-panel">{diffText}</pre>}
            <div className="file-editor-status">
              <span>{dirty ? t("Unsaved changes") : t("No unsaved changes")} · {openedFile.language ?? "Text"} · {openedFile.encoding ?? encoding} · {formatSize(openedFile.size ?? content.length)} · {editorMatches} {t("matches")}</span>
              {jsonError && <strong className="danger-text">{jsonError}</strong>}
              {codeDiagnostics.map((item) => <strong key={item} className="danger-text">{item}</strong>)}
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
  history,
  onActivate,
  onRefresh,
  onHistory,
  onFilter,
  onSort,
  onSelectAll,
  onInvertSelection,
  onSelect,
  onOpen,
  onContextMenu,
  onDropFiles,
  onDropPaths,
  onCopyPath
}: {
  paneKey: PaneKey;
  pane: PaneState;
  active: boolean;
  selectedPaths: string[];
  roots: Array<{ label: string; path: string }>;
  history: { back: string[]; forward: string[] };
  onActivate: () => void;
  onRefresh: (path?: string) => void;
  onHistory: (direction: "back" | "forward") => void;
  onFilter: (filter: string) => void;
  onSort: (pane: PaneKey, sortKey: PaneState["sortKey"]) => void;
  onSelectAll: (pane: PaneKey) => void;
  onInvertSelection: (pane: PaneKey) => void;
  onSelect: (pane: PaneKey, entry: FileEntry, event?: MouseEvent) => void;
  onOpen: (entry: FileEntry) => void;
  onContextMenu: (pane: PaneKey, entry: FileEntry, event: MouseEvent) => void;
  onDropFiles: (pane: PaneKey, files: FileList) => void;
  onDropPaths: (pane: PaneKey, paths: string[], mode: "copy" | "move") => void;
  onCopyPath: (path: string) => void;
}) {
  const t = useT();
  const filteredEntries = useMemo(() => {
    const query = pane.filter.trim().toLowerCase();
    const entries = query ? pane.entries.filter((entry) => entry.name.toLowerCase().includes(query)) : pane.entries;
    return [...entries].sort((a, b) => {
      const dir = pane.sortDir === "asc" ? 1 : -1;
      if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
      if (pane.sortKey === "size") return (a.size - b.size) * dir;
      if (pane.sortKey === "modified") return ((a.modified ?? "").localeCompare(b.modified ?? "")) * dir;
      return a.name.localeCompare(b.name) * dir;
    });
  }, [pane.entries, pane.filter, pane.sortDir, pane.sortKey]);

  const visibleEntries = filteredEntries.slice(0, 500);

  return (
    <div
      className={`file-pane ${active ? "active" : ""}`}
      onFocus={onActivate}
      onClick={onActivate}
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        onActivate();
        const rawPaths = event.dataTransfer.getData("application/localstack-paths");
        if (rawPaths) {
          try {
            const paths = JSON.parse(rawPaths);
            if (Array.isArray(paths)) onDropPaths(paneKey, paths.filter((path) => typeof path === "string"), event.ctrlKey ? "copy" : "move");
          } catch {
            // Ignore malformed drag data.
          }
          return;
        }
        if (event.dataTransfer.files.length) onDropFiles(paneKey, event.dataTransfer.files);
      }}
    >
      <div className="file-pane-head">
        <strong>{paneKey === "left" ? t("Left Pane") : t("Right Pane")}</strong>
        <div className="toolbar">
          <Button variant="icon" icon={<ArrowLeft size={16} />} disabled={!history.back.length} onClick={() => onHistory("back")} aria-label="Back" />
          <Button variant="icon" icon={<ArrowRight size={16} />} disabled={!history.forward.length} onClick={() => onHistory("forward")} aria-label="Forward" />
          <Button variant="icon" icon={<CheckSquare size={16} />} onClick={() => onSelectAll(paneKey)} aria-label="Select all" />
          <Button variant="icon" icon={<Replace size={16} />} onClick={() => onInvertSelection(paneKey)} aria-label="Invert selection" />
          <Button variant="icon" icon={<RefreshCw size={16} />} onClick={() => onRefresh()} aria-label="Refresh" />
        </div>
      </div>
      <div className="file-breadcrumbs">
        {breadcrumbs(pane.folder).map((crumb) => (
          <button key={crumb.path} onClick={() => onRefresh(crumb.path)}>{crumb.label}</button>
        ))}
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
        <span>{visibleEntries.length}/{filteredEntries.length} / {pane.entries.length}</span>
      </div>
      <div className="file-sortbar">
        <button onClick={() => onSort(paneKey, "name")}>{t("Name")} {pane.sortKey === "name" ? pane.sortDir : ""}</button>
        <button onClick={() => onSort(paneKey, "size")}>{t("Size")} {pane.sortKey === "size" ? pane.sortDir : ""}</button>
        <button onClick={() => onSort(paneKey, "modified")}>{t("Modified")} {pane.sortKey === "modified" ? pane.sortDir : ""}</button>
      </div>
      <div className="file-list pane-list">
        {visibleEntries.map((entry) => (
          <button
            key={entry.path}
            className={`file-row ${selectedPaths.includes(entry.path) ? "selected" : ""}`}
            draggable
            onDragStart={(event) => {
              const paths = selectedPaths.includes(entry.path) ? selectedPaths : [entry.path];
              event.dataTransfer.setData("application/localstack-paths", JSON.stringify(paths));
              event.dataTransfer.effectAllowed = "copyMove";
            }}
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
        {filteredEntries.length > visibleEntries.length && <div className="empty-row">{filteredEntries.length - visibleEntries.length} more item(s). Narrow the file-name search.</div>}
        {!filteredEntries.length && <div className="empty-row">{t("No files found.")}</div>}
      </div>
    </div>
  );
}

function paneState(folder: string): PaneState {
  return { folder, entries: [], selected: null, filter: "", sortKey: "name", sortDir: "asc" };
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

function breadcrumbs(path: string) {
  const normalized = path.replace(/[\\/]+$/, "");
  const parts = normalized.split(/[\\/]/).filter(Boolean);
  if (!parts.length) return [{ label: normalized || "/", path: normalized || "/" }];
  const root = normalized.match(/^[A-Za-z]:/)?.[0];
  const crumbs: Array<{ label: string; path: string }> = [];
  let current = root ?? "";
  if (root) crumbs.push({ label: root, path: `${root}\\` });
  const rest = root ? parts.slice(1) : parts;
  for (const part of rest) {
    current = current ? `${current}\\${part}` : part;
    crumbs.push({ label: part, path: current });
  }
  return crumbs.slice(-6);
}

function loadStringList(key: string) {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(value) ? value.filter((item) => typeof item === "string") : [];
  } catch {
    return [];
  }
}

function loadJsonList<T>(key: string) {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(value) ? value as T[] : [];
  } catch {
    return [];
  }
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

function renameWithPattern(name: string, find: string, replacement: string, regexp: boolean) {
  if (!find) return name;
  try {
    return regexp ? name.replace(new RegExp(find, "g"), replacement) : name.split(find).join(replacement);
  } catch {
    return name;
  }
}

function validateJson(content: string, languageOrPath: string) {
  const value = languageOrPath.toLowerCase();
  if (!value.includes("json") && !value.endsWith(".json")) return "";
  try {
    JSON.parse(content);
    return "";
  } catch (error) {
    return error instanceof Error ? error.message : "Invalid JSON";
  }
}

function quickCodeDiagnostics(content: string, languageOrPath: string) {
  const value = languageOrPath.toLowerCase();
  const diagnostics: string[] = [];
  if ((value.includes("php") || value.endsWith(".php")) && !content.includes("<?php") && !content.includes("<?= ")) {
    diagnostics.push("PHP opening tag is missing.");
  }
  const open = (content.match(/[({[]/g) ?? []).length;
  const close = (content.match(/[)}\]]/g) ?? []).length;
  if ((value.includes("javascript") || value.includes("typescript") || value.endsWith(".js") || value.endsWith(".ts") || value.endsWith(".tsx")) && Math.abs(open - close) > 0) {
    diagnostics.push("Bracket count looks unbalanced.");
  }
  return diagnostics;
}

function formatEditorContent(content: string, languageOrPath: string) {
  const value = languageOrPath.toLowerCase();
  try {
    if (value.includes("json") || value.endsWith(".json")) return `${JSON.stringify(JSON.parse(content), null, 2)}\n`;
    if (value.includes("css") || value.endsWith(".css") || value.endsWith(".scss")) return formatBracedText(content);
    if (value.includes("html") || value.endsWith(".html") || value.endsWith(".htm")) return formatHtml(content);
  } catch {
    return content;
  }
  return content;
}

function formatBracedText(content: string) {
  let level = 0;
  return content
    .replace(/\s*([{};])\s*/g, "$1\n")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      if (line.startsWith("}")) level = Math.max(0, level - 1);
      const output = `${"  ".repeat(level)}${line}`;
      if (line.endsWith("{")) level += 1;
      return output;
    })
    .join("\n")
    .concat("\n");
}

function formatHtml(content: string) {
  let level = 0;
  return content
    .replace(/>\s*</g, ">\n<")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      if (/^<\//.test(line)) level = Math.max(0, level - 1);
      const output = `${"  ".repeat(level)}${line}`;
      if (/^<[^/!][^>]*[^/]>\s*$/.test(line) && !/^<(input|img|br|hr|meta|link)\b/i.test(line)) level += 1;
      return output;
    })
    .join("\n")
    .concat("\n");
}

function buildLineDiff(leftName: string, left: string, rightName: string, right: string) {
  const leftLines = left.split(/\r?\n/);
  const rightLines = right.split(/\r?\n/);
  const max = Math.max(leftLines.length, rightLines.length);
  const output = [`--- ${leftName}`, `+++ ${rightName}`];
  for (let index = 0; index < max; index += 1) {
    const a = leftLines[index] ?? "";
    const b = rightLines[index] ?? "";
    if (a === b) continue;
    output.push(`-${index + 1}: ${a}`);
    output.push(`+${index + 1}: ${b}`);
    if (output.length > 260) {
      output.push("Diff truncated.");
      break;
    }
  }
  return output.length > 2 ? output.join("\n") : "No differences.";
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
