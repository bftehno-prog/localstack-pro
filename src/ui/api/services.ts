import { api } from "../api";

export const servicesApi = {
  startAll: api.startAll,
  stopAll: api.stopAll,
  restartAll: api.restartAll,
  startService: api.startService,
  startServiceProfile: api.startServiceProfile,
  stopService: api.stopService,
  restartService: api.restartService,
  saveService: api.saveService,
  installServiceDependency: api.installServiceDependency,
  installAllMissingDependencies: api.installAllMissingDependencies,
  detectDependencies: api.detectDependencies
};
