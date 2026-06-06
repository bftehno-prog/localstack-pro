import type { AppSnapshot, HostInfo } from "./types";

export function hostReadinessScore(state: AppSnapshot, host: HostInfo) {
  const serviceId = host.webServer.toLowerCase().includes("nginx") ? "nginx" : "apache";
  const webService = state.services.find((service) => service.id === serviceId);
  const php = state.phpVersions.some((item) => item.version === host.phpVersion);
  const databaseReady = !host.database || state.databases.some((database) => database.name === host.database || database.id === host.database);
  const sslReady = !host.ssl || state.certificates.some((certificate) => certificate.domain === host.domain && certificate.status !== "Invalid");
  const cmsReady = !host.tags.includes("cms") || state.cmsInstallations.some((cms) => cms.domain === host.domain && cms.status === "installed");
  const parts = [
    webService?.status === "running" ? 25 : 0,
    host.status === "running" ? 20 : 0,
    php ? 15 : 0,
    databaseReady ? 15 : 0,
    sslReady ? 15 : 0,
    cmsReady ? 10 : 0
  ];
  return parts.reduce((sum, value) => sum + value, 0);
}

export function readinessLabel(score: number) {
  if (score >= 85) return "Ready";
  if (score >= 60) return "Needs attention";
  return "Not ready";
}

export function readinessClass(score: number) {
  if (score >= 85) return "green";
  if (score >= 60) return "orange";
  return "red";
}
