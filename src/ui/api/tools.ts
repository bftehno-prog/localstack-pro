import { api } from "../api";

export const toolsApi = {
  scanPorts: api.scanPorts,
  runProjectCommand: api.runProjectCommand,
  cloneProjectRepository: api.cloneProjectRepository,
  inspectProject: api.inspectProject,
  generateEnvTemplate: api.generateEnvTemplate,
  checkLatestRelease: api.checkLatestRelease,
  downloadLatestReleaseInstaller: api.downloadLatestReleaseInstaller,
  installDownloadedUpdate: api.installDownloadedUpdate,
  readConfigFile: api.readConfigFile,
  saveConfigFile: api.saveConfigFile,
  createDiagnosticBundle: api.createDiagnosticBundle,
  diagnoseSsl: api.diagnoseSsl,
  inspectInstalledTools: api.inspectInstalledTools,
  listEnvironmentSnapshots: api.listEnvironmentSnapshots,
  createEnvironmentSnapshot: api.createEnvironmentSnapshot,
  restoreEnvironmentSnapshot: api.restoreEnvironmentSnapshot,
  listNodeScripts: api.listNodeScripts,
  runNodeScript: api.runNodeScript,
  resourceMonitor: api.resourceMonitor,
  killProcess: api.killProcess
};
