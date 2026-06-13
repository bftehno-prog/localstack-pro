import { api } from "../api";

export const filesApi = {
  listFiles: api.listFiles,
  readFile: api.readFile,
  readFileWithEncoding: api.readFileWithEncoding,
  writeFile: api.writeFile,
  writeFileWithEncoding: api.writeFileWithEncoding,
  createFile: api.createFile,
  createFolder: api.createFolder,
  deletePath: api.deletePath,
  trashPath: api.trashPath,
  restoreTrashPath: api.restoreTrashPath,
  renamePath: api.renamePath,
  duplicatePath: api.duplicatePath,
  copyPath: api.copyPath,
  movePath: api.movePath,
  chmodPath: api.chmodPath,
  uploadFiles: api.uploadFiles,
  extractArchiveTo: api.extractArchiveTo,
  createArchive: api.createArchive,
  searchFileContents: api.searchFileContents,
  searchFileContentsAdvanced: api.searchFileContentsAdvanced,
  listArchiveEntries: api.listArchiveEntries,
  applyWindowsAcl: api.applyWindowsAcl
};
