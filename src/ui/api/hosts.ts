import { api } from "../api";

export const hostsApi = {
  saveHost: api.saveHost,
  deleteHost: api.deleteHost,
  duplicateHost: api.duplicateHost,
  syncHostsFile: api.syncHostsFile,
  diagnoseHost: api.diagnoseHost,
  repairHost: api.repairHost,
  importHosts: api.importHosts,
  exportHosts: api.exportHosts,
  openHost: api.openHost,
  previewHost: api.previewHost,
  exportPortableHost: api.exportPortableHost,
  backupHost: api.backupHost,
  restoreHostBackup: api.restoreHostBackup
};
