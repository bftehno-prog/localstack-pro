import { api } from "../api";

export const settingsApi = {
  saveSettings: api.saveSettings,
  exportSettings: api.exportSettings,
  importSettings: api.importSettings,
  resetSettings: api.resetSettings,
  createAppBackup: api.createAppBackup,
  restoreAppBackup: api.restoreAppBackup,
  openCertificateStore: api.openCertificateStore,
  openDocumentation: api.openDocumentation,
  openPath: api.openPath,
  openUrl: api.openUrl,
  openTerminal: api.openTerminal,
  openDatabaseAdmin: api.openDatabaseAdmin,
  hideTrayPanel: api.hideTrayPanel,
  openMainPage: api.openMainPage,
  quit: api.quit
};
