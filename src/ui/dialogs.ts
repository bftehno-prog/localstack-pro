import { open, save } from "@tauri-apps/plugin-dialog";

export async function pickJsonFile() {
  const selected = await open({
    multiple: false,
    filters: [{ name: "JSON", extensions: ["json"] }]
  });
  return typeof selected === "string" ? selected : undefined;
}

export async function saveJsonFile(defaultPath: string) {
  const selected = await save({
    defaultPath,
    filters: [{ name: "JSON", extensions: ["json"] }]
  });
  return selected ?? undefined;
}

export async function pickSqlFile() {
  const selected = await open({
    multiple: false,
    filters: [{ name: "SQL", extensions: ["sql"] }]
  });
  return typeof selected === "string" ? selected : undefined;
}

export async function saveTextFile(defaultPath: string) {
  const selected = await save({
    defaultPath,
    filters: [{ name: "Text", extensions: ["txt", "log"] }]
  });
  return selected ?? undefined;
}

export async function pickZipFile() {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "ZIP Archive", extensions: ["zip"] }]
  });
  return typeof selected === "string" ? selected : null;
}

export async function saveZipFile(defaultPath: string) {
  const selected = await save({
    defaultPath,
    filters: [{ name: "ZIP Archive", extensions: ["zip"] }]
  });
  return selected ?? null;
}

export async function pickFolder() {
  const selected = await open({
    multiple: false,
    directory: true
  });
  return typeof selected === "string" ? selected : undefined;
}
