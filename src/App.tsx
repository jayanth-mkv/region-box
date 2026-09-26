// Standard shadcn composition, per the user's explicit brief.
// Workspace list -> select a country -> start and use its browser.
// The browser stage owns the available space; settings and errors stay inline.
import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Boxes, Globe, Play, Square, Plus, RefreshCw, Settings, LoaderCircle, ShieldCheck, CircleAlert, ChevronLeft, Maximize, Minimize } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Onboarding } from '@/components/onboarding';
import { NordCredentialsGuide } from '@/components/nord-credentials-guide';
import { AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction } from '@/components/ui/alert-dialog';
import { countries, type SetupStatus, type Snapshot, type Workspace } from './types';

const stateLabel: Record<Workspace['state'], string> = {
  stopped: 'Stopped', starting: 'Starting', stopping: 'Stopping', running: 'Running',
  error: 'Needs attention', unavailable: 'Docker unavailable', unhealthy: 'Connection not ready',
};
const isBusyState = (workspace: Workspace) => ['starting', 'stopping'].includes(workspace.state);

export default function App() {
  const [data, setData] = useState<Snapshot | null>(null);
  const [selected, setSelected] = useState('us');
  const [tab, setTab] = useState('workspace');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState('');
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('');
  const [country, setCountry] = useState('US');
  const [user, setUser] = useState('');
  const [password, setPassword] = useState('');
  const [saved, setSaved] = useState(false);
  const [logs, setLogs] = useState<string | null>(null);
  const [frameVersion, setFrameVersion] = useState(0);
  const [closing, setClosing] = useState(false);
  const [setup, setSetup] = useState<SetupStatus | null>(null);
  const [showSetup, setShowSetup] = useState(false);
  const [setupWorking, setSetupWorking] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [resizing, setResizing] = useState(false);
  const refreshing = useRef(false);
  const current = data?.workspaces.find(w => w.id === selected) ?? data?.workspaces[0];
  const running = data?.workspaces.filter(w => w.state === 'running').length ?? 0;
  const working = !!busy || !!data?.workspaces.some(isBusyState);

  const refresh = useCallback(async () => {
    if (!isTauri() || refreshing.current) return;
    refreshing.current = true;
    try { setData(await invoke<Snapshot>('snapshot')); }
    catch (e) { setError(String(e)); }
    finally { refreshing.current = false; }
  }, []);

  useEffect(() => {
    void refresh();
    if (isTauri()) void invoke<SetupStatus>('setup_status').then(status => { setSetup(status); setShowSetup(!status.dismissed); }).catch(e => setError(String(e)));
    const timer = window.setInterval(() => void refresh(), 4000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (!isTauri()) return;
    const unlisten = getCurrentWindow().onCloseRequested(event => { event.preventDefault(); setClosing(true); });
    return () => { void unlisten.then(fn => fn()); };
  }, []);

  const action = async (label: string, fn: () => Promise<unknown>) => {
    setBusy(label); setError(''); setSaved(false);
    try { await fn(); }
    catch (e) { setError(String(e)); }
    finally { setBusy(''); await refresh(); }
  };

  const toggleFullscreen = async () => {
    setResizing(true);
    try {
      const window = getCurrentWindow();
      const next = !(await window.isFullscreen());
      await window.setFullscreen(next);
      setFullscreen(next);
    } catch (e) { setError(String(e)); }
    finally { setResizing(false); }
  };

  const create = (event: FormEvent) => {
    event.preventDefault();
    void action('Creating workspace', async () => {
      const workspace = await invoke<Workspace>('create_workspace', { name, country });
      setSelected(workspace.id); setCreating(false); setName(''); setTab('workspace');
    });
  };

  const save = (event: FormEvent) => {
    event.preventDefault();
    void action('Checking NordVPN credentials', async () => {
      await invoke('save_credentials', { user, password });
      setUser(''); setPassword(''); setSaved(true);
    });
  };

  const start = () => {
    if (current) void action('Starting workspace', () => invoke('start_workspace', { id: current.id }));
  };

  const openSetup = () => void action('Checking setup', async () => {
    setSetup(await invoke<SetupStatus>('setup_status')); setShowSetup(true);
  });

  const closeDialog = <AlertDialog open={closing} onOpenChange={setClosing}>
    <AlertDialogContent className="sm:max-w-lg"><AlertDialogHeader><AlertDialogTitle>Close RegionBox?</AlertDialogTitle><AlertDialogDescription>{setupWorking ? 'A setup step is running. Wait for it to finish before closing RegionBox.' : 'You can stop all workspaces now, or leave them running in Docker. Browser data is saved either way.'}</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel>Cancel</AlertDialogCancel><Button variant="outline" disabled={working || setupWorking} onClick={() => void action('Closing RegionBox', () => invoke('quit_app', { stop: false }))}>Keep running and exit</Button><AlertDialogAction disabled={working || setupWorking} onClick={event => { event.preventDefault(); void action('Stopping workspaces', () => invoke('quit_app', { stop: true })); }}>Stop all and exit</AlertDialogAction></AlertDialogFooter>{error && <p role="alert" className="text-sm text-destructive">{error}</p>}</AlertDialogContent>
  </AlertDialog>;

  if (!isTauri()) return (
    <main className="grid min-h-svh place-items-center bg-muted/30 p-6">
      <Card className="w-full max-w-lg">
        <CardHeader><CardTitle>Open RegionBox on your desktop</CardTitle><CardDescription>The browser preview cannot manage your local workspaces.</CardDescription></CardHeader>
        <CardContent><p className="text-sm">Launch RegionBox.exe, or run <code className="rounded bg-muted px-1.5 py-1">npm run desktop</code> from this project.</p></CardContent>
      </Card>
    </main>
  );

  if (!setup?.checked || !data) return <><main className="grid h-svh place-items-center p-6"><Card className="w-full max-w-lg"><CardHeader><CardTitle>Opening RegionBox</CardTitle><CardDescription>{error || 'Checking this PC and your saved workspaces…'}</CardDescription></CardHeader>{error && <CardContent><Button onClick={() => window.location.reload()}>Try again</Button></CardContent>}</Card></main>{closeDialog}</>;

  if (showSetup) return <><Onboarding initial={setup} data={data} refresh={refresh} onBusy={setSetupWorking} onLeave={id => { setShowSetup(false); if (id) { setSelected(id); setTab('workspace'); } }} />{closeDialog}</>;

  return (
    <div data-fullscreen={fullscreen} className="flex h-svh min-w-0 flex-col overflow-hidden bg-background text-foreground [&[data-fullscreen=true]_.app-chrome]:hidden">
      <header className="app-chrome flex shrink-0 items-center justify-between gap-4 border-b px-5 py-3">
        <div className="flex items-center gap-2.5"><Boxes className="size-5" /><span className="font-semibold">RegionBox</span><Badge variant="secondary">Local</Badge></div>
        <div className="flex items-center gap-3">
          <span className="hidden text-sm text-muted-foreground sm:inline">{running} running</span>
          <Button variant="outline" size="sm" disabled={working || !data?.dockerReady} onClick={() => void action('Stopping all workspaces', () => invoke('stop_all'))}><Square />Stop all</Button>
          <Button variant="ghost" size="icon-sm" aria-label="Refresh status" disabled={refreshing.current} onClick={() => void refresh()}><RefreshCw /></Button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1 flex-col sm:flex-row">
        <aside className="app-chrome flex min-h-0 shrink-0 flex-col border-b bg-muted/20 sm:w-60 sm:border-r sm:border-b-0">
          <div className="flex items-center justify-between px-4 pt-5 pb-3"><h1 className="text-sm font-medium">Workspaces</h1><Button size="icon-sm" variant="ghost" aria-label="Create workspace" disabled={working} onClick={() => { setCreating(true); setTab('workspace'); setLogs(null); }}><Plus /></Button></div>
          <ScrollArea className="max-h-44 sm:max-h-none sm:flex-1">
            <nav aria-label="Workspaces" className="space-y-1 px-2 pb-3">
              {data?.workspaces.map(workspace => (
                <Button key={workspace.id} variant={workspace.id === selected && tab === 'workspace' && !creating ? 'secondary' : 'ghost'} className="h-auto w-full justify-start gap-3 px-3 py-3 text-left" aria-current={workspace.id === selected ? 'page' : undefined} onClick={() => { setSelected(workspace.id); setCreating(false); setTab('workspace'); setLogs(null); }}>
                  <span className="text-xs font-semibold text-muted-foreground">{workspace.country}</span>
                  <span className="min-w-0 flex-1"><span className="block truncate text-sm">{workspace.name}</span><span className="mt-0.5 block text-xs font-normal text-muted-foreground">{stateLabel[workspace.state]}</span></span>
                  {isBusyState(workspace) ? <LoaderCircle className="size-3.5 animate-spin motion-reduce:animate-none" /> : workspace.state === 'running' ? <span className="size-2 rounded-full bg-emerald-600" aria-hidden="true" /> : null}
                </Button>
              ))}
              {!data && <p className="px-3 py-4 text-sm text-muted-foreground">Checking your workspaces…</p>}
            </nav>
          </ScrollArea>
          <div className="border-t p-3">
            <Button className="w-full justify-start" variant={tab === 'settings' ? 'secondary' : 'ghost'} onClick={() => { setTab('settings'); setCreating(false); }}><Settings />Settings</Button>
            <p className="mt-2 px-3 text-xs text-muted-foreground">{data?.dockerReady ? 'Docker is ready' : data ? 'Docker needs attention' : 'Checking Docker…'}</p>
          </div>
        </aside>

        <main className="flex min-h-0 min-w-0 flex-1 flex-col">
          {error && <div className="shrink-0 px-5 pt-4"><Alert variant="destructive"><CircleAlert /><AlertTitle>Could not complete the action</AlertTitle><AlertDescription><p className="whitespace-pre-wrap break-words">{error}</p><Button size="sm" variant="outline" onClick={() => setError('')}>Dismiss</Button></AlertDescription></Alert></div>}
          {data && !data.dockerReady && <div className="shrink-0 px-5 pt-4"><Alert><CircleAlert /><AlertTitle>Set up Docker Desktop</AlertTitle><AlertDescription><p>{data.dockerMessage}</p><Button size="sm" disabled={working} onClick={openSetup}>Open setup</Button></AlertDescription></Alert></div>}

          <Tabs value={tab} onValueChange={setTab} className="flex min-h-0 flex-1 flex-col gap-0">
            <div className="app-chrome shrink-0 px-5 pt-4"><TabsList><TabsTrigger value="workspace">Browser</TabsTrigger><TabsTrigger value="settings">Settings</TabsTrigger></TabsList></div>

            <TabsContent value="settings" className="min-h-0 flex-1 overflow-auto p-5">
              <div className="max-w-xl space-y-5">
                <Card>
                  <CardHeader><CardTitle>NordVPN connection</CardTitle></CardHeader>
                  <CardContent>
                    <div className="mb-5 flex flex-wrap items-center gap-2"><Badge variant={data?.credentialsVerified ? 'default' : 'secondary'}>{data?.credentialsVerified ? 'Credentials verified' : data?.credentialsReady ? 'Credentials saved · check needed' : 'Credentials needed'}</Badge>{data?.credentialsReady && <Button size="sm" variant="outline" disabled={working} onClick={() => void action('Checking NordVPN credentials', () => invoke('check_saved_credentials'))}>Check saved credentials</Button>}</div>
                    <div className="mb-5"><NordCredentialsGuide onOpen={() => void invoke('open_setup_help', { topic: 'nord' }).catch(e => setError(String(e)))} /></div>
                    <form onSubmit={save} className="space-y-4">
                      <div className="space-y-2"><Label htmlFor="service-user">Service username</Label><Input id="service-user" disabled={working} value={user} onChange={e => setUser(e.target.value)} autoComplete="off" spellCheck={false} required placeholder="Your NordVPN service username" /></div>
                      <div className="space-y-2"><Label htmlFor="service-password">Service password</Label><Input id="service-password" disabled={working} type="password" value={password} onChange={e => setPassword(e.target.value)} autoComplete="new-password" required placeholder="Your NordVPN service password" /></div>
                      <Button type="submit" disabled={working || !user.trim() || !password.trim()}>Check and save credentials</Button>
                      {busy === 'Checking NordVPN credentials' && <p role="status" className="text-sm text-muted-foreground">Checking your credentials with NordVPN. Trying recommended servers can take a few minutes.</p>}
                      {saved && <p role="status" className="text-sm">Credentials verified and saved. You can now start a workspace.</p>}
                    </form>
                  </CardContent>
                </Card>
                <Card><CardHeader><CardTitle>Setup & checks</CardTitle><CardDescription>Prepare Windows, start Docker, download browser files, and check your first connection.</CardDescription></CardHeader><CardContent><Button variant="outline" disabled={working} onClick={openSetup}>Open setup</Button></CardContent></Card>
                <Card><CardHeader><CardTitle>Browser data</CardTitle><CardDescription>Each workspace keeps its own cookies, history, and downloads in a Docker volume. Stopping a workspace preserves this data.</CardDescription></CardHeader><CardContent><p className="text-sm text-muted-foreground">Workspace settings</p><p className="mt-2 break-all font-mono text-xs">{data?.dataPath}</p></CardContent></Card>
              </div>
            </TabsContent>

            <TabsContent value="workspace" className="flex min-h-0 flex-1 flex-col">
              {creating ? <div className="overflow-auto p-5"><Card className="max-w-lg"><CardHeader><CardTitle>Create workspace</CardTitle><CardDescription>A separate Chromium profile and NordVPN connection.</CardDescription></CardHeader><CardContent><form onSubmit={create} className="space-y-4"><div className="space-y-2"><Label htmlFor="workspace-name">Name</Label><Input id="workspace-name" value={name} onChange={e => setName(e.target.value)} maxLength={48} required autoFocus placeholder="e.g. Germany work" /></div><div className="space-y-2"><Label htmlFor="workspace-country">Country</Label><NativeSelect id="workspace-country" value={country} onChange={e => setCountry(e.target.value)}>{Object.entries(countries).map(([code, label]) => <NativeSelectOption key={code} value={code}>{label}</NativeSelectOption>)}</NativeSelect></div><div className="flex gap-2"><Button type="submit" disabled={working || !name.trim()}>Create workspace</Button><Button variant="outline" type="button" onClick={() => setCreating(false)}>Cancel</Button></div></form></CardContent></Card></div> : current ? <>
                <div className={`flex shrink-0 flex-wrap items-center justify-between gap-3 ${fullscreen ? 'px-4 py-2' : 'p-5'}`}>
                  <div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><h2 className="max-w-96 truncate text-lg font-semibold">{current.name}</h2><Badge variant={current.state === 'running' ? 'default' : 'secondary'}>{stateLabel[current.state]}</Badge></div><p className="app-chrome mt-1 text-sm text-muted-foreground">{countries[current.country]} · Chromium</p></div>
                  <div className="flex flex-wrap gap-2">
                    <Button variant="outline" size="sm" disabled={resizing} onClick={() => void toggleFullscreen()}>{fullscreen ? <Minimize /> : <Maximize />}{fullscreen ? 'Exit full screen' : 'Full screen'}</Button>
                    <Button variant="outline" size="sm" disabled={working || current.state !== 'running'} onClick={() => void action('Checking IP', () => invoke('verify_workspace', { id: current.id }))}><ShieldCheck />Check IP</Button>
                    {current.state === 'running' || current.state === 'unhealthy' || current.state === 'error' ? <Button variant="outline" size="sm" disabled={working || !data?.dockerReady} onClick={() => void action('Stopping workspace', () => invoke('stop_workspace', { id: current.id }))}><Square />Stop</Button> : null}
                    {current.state !== 'running' && <Button size="sm" disabled={working || !data?.dockerReady || !data.credentialsReady} onClick={start}>{working ? <LoaderCircle className="animate-spin motion-reduce:animate-none" /> : <Play />}{current.state === 'unhealthy' || current.state === 'error' ? 'Retry start' : 'Start workspace'}</Button>}
                  </div>
                </div>

                {current.network && <div className="app-chrome flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-y bg-muted/30 px-5 py-2 text-xs"><span>Public IP <span className="ml-1 font-mono font-medium">{current.network.ip}</span></span><span>Detected country <strong>{countries[current.network.country] ?? current.network.country}</strong></span><span className="text-muted-foreground">Checked {new Date(current.network.checkedAt * 1000).toLocaleTimeString()}</span>{current.network.country !== current.country && <span className="font-medium text-destructive">Country differs from your selection. Restart to try another server.</span>}</div>}

                <div className={`relative min-h-0 min-w-0 flex-1 border-t bg-muted/20 ${current.browserUrl && logs === null ? 'overflow-hidden' : 'overflow-auto'}`}>
                  {current.browserUrl ? logs !== null ? <div className="p-5"><Button size="sm" variant="outline" className="mb-4" onClick={() => setLogs(null)}><ChevronLeft />Back to browser</Button><pre className="whitespace-pre-wrap break-all rounded-md border bg-background p-4 text-xs">{logs}</pre></div> : <iframe key={`${current.id}-${frameVersion}`} title={`${current.name} browser`} src={current.browserUrl} className="absolute inset-0 block h-full min-h-0 w-full border-0 bg-background" allow="clipboard-read; clipboard-write; fullscreen" referrerPolicy="no-referrer" /> : <div className="grid min-h-full place-items-center p-6">
                    <div className="w-full max-w-md space-y-5">
                      {isBusyState(current) ? <><LoaderCircle className="size-8 animate-spin text-muted-foreground motion-reduce:animate-none" /><h3 className="text-lg font-medium">{stateLabel[current.state]}</h3><p role="status" className="text-sm text-muted-foreground">{current.detail}</p></> : !data?.credentialsReady ? <><Globe className="size-8 text-muted-foreground" /><h3 className="text-lg font-medium">Connect NordVPN to get started</h3><p className="text-sm leading-6 text-muted-foreground">Your US, Germany, and UK workspaces are ready to set up. Add your service credentials, then start the browsers you need.</p><Button onClick={() => setTab('settings')}><Settings />Set up NordVPN</Button></> : <><Globe className="size-8 text-muted-foreground" /><h3 className="text-lg font-medium">{current.state === 'error' || current.state === 'unhealthy' ? 'This workspace needs attention' : 'Your browser is stopped'}</h3><p className="whitespace-pre-wrap break-words text-sm leading-6 text-muted-foreground">{current.detail}</p><p className="text-sm text-muted-foreground">Start this workspace to browse through {countries[current.country]}. Your other workspaces stay independent.</p><Button disabled={working || !data.dockerReady} onClick={start}><Play />Start workspace</Button></>}
                      {logs !== null && <pre className="max-h-60 overflow-auto whitespace-pre-wrap break-all rounded-md border bg-background p-3 text-xs">{logs}</pre>}
                    </div>
                  </div>}
                </div>
                <footer className="app-chrome flex shrink-0 flex-wrap items-center justify-between gap-2 border-t px-5 py-2">
                  <p className="text-xs text-muted-foreground">{current.state === 'running' && !current.network ? 'Country and public IP have not been verified yet.' : 'Stopping a workspace keeps its browser data.'}</p>
                  <div className="flex gap-1">{current.browserUrl && <Button variant="ghost" size="sm" onClick={() => { setLogs(null); setFrameVersion(v => v + 1); }}><RefreshCw />Reload view</Button>}<Button variant="ghost" size="sm" disabled={working} onClick={() => void action('Loading logs', async () => setLogs(await invoke<string>('workspace_logs', { id: current.id })))}>View logs</Button></div>
                </footer>
              </> : <div className="p-6 text-sm text-muted-foreground">Loading workspace settings…</div>}
            </TabsContent>
          </Tabs>
        </main>
      </div>

      {closeDialog}
      <span role="status" className="sr-only">{busy}</span>
    </div>
  );
}
