export type Workspace = {
  id: string; name: string; country: string; port: number;
  state: 'stopped' | 'starting' | 'running' | 'stopping' | 'unhealthy' | 'error' | 'unavailable';
  detail: string; browserUrl: string | null;
  network: { ip: string; country: string; checkedAt: number } | null;
};
export type Snapshot = {
  dockerReady: boolean; dockerMessage: string; credentialsReady: boolean; credentialsVerified: boolean;
  credentialsPath: string; dataPath: string; workspaces: Workspace[];
};
export const countries: Record<string, string> = {
  IN: 'India', US: 'United States', GB: 'United Kingdom', AU: 'Australia', CA: 'Canada',
  AE: 'United Arab Emirates', FR: 'France', SG: 'Singapore', DE: 'Germany', NL: 'Netherlands',
};

export type SetupStatus = {
  checked: boolean; supported: boolean; virtualization: boolean; windowsReady: boolean;
  dockerInstalled: boolean; dockerReady: boolean; vpnImageReady: boolean; browserImageReady: boolean;
  dismissed: boolean; restartRequired: boolean; busy: boolean; phase: string; detail: string;
  error: string | null; downloaded: number; total: number | null;
};
