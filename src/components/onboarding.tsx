import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Boxes, Check, CircleAlert, ExternalLink, LoaderCircle, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select';
import { Progress } from '@/components/ui/progress';
import { countries, type SetupStatus, type Snapshot } from '@/types';
import { NordCredentialsGuide } from '@/components/nord-credentials-guide';

type Props = { initial: SetupStatus; data: Snapshot; refresh: () => Promise<void>; onLeave: (workspaceId?: string) => void; onBusy: (busy: boolean) => void };
const steps = ['Windows support', 'Docker Desktop', 'Browser files', 'NordVPN', 'First browser'];

export function Onboarding({ initial, data, refresh, onLeave, onBusy }: Props) {
  const [setup, setSetup] = useState(initial);
  const [error, setError] = useState('');
  const [pending, setPending] = useState(false);
  const [checking, setChecking] = useState(false);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [workspaceId, setWorkspaceId] = useState(data.workspaces[0]?.id ?? 'us');
  const [changeCredentials, setChangeCredentials] = useState(false);
  const polling = useRef(false);
  const selected = data.workspaces.find(workspace => workspace.id === workspaceId);
  const connected = selected?.state === 'running';
  const verified = connected && selected.network?.country === selected.country;
  const activeWorkspace = data.workspaces.find(workspace => ['starting', 'stopping'].includes(workspace.state));
  const busy = pending || setup.busy || !!activeWorkspace;
  const ready = [setup.windowsReady, setup.dockerReady, setup.vpnImageReady && setup.browserImageReady, data.credentialsReady && !changeCredentials, !!verified];
  const complete = ready.every(Boolean);
  const next = ready.findIndex(value => !value);
  const step = next === -1 ? 4 : next;

  const check = useCallback(async () => {
    if (polling.current) return;
    polling.current = true; setChecking(true);
    try { setSetup(await invoke<SetupStatus>('setup_status')); }
    catch (e) { setError(String(e)); }
    finally { polling.current = false; setChecking(false); }
  }, []);
  useEffect(() => {
    const timer = window.setInterval(() => void check(), busy ? 2000 : 10000);
    return () => window.clearInterval(timer);
  }, [check, busy]);
  useEffect(() => { onBusy(busy); return () => onBusy(false); }, [busy, onBusy]);

  const run = async (work: () => Promise<unknown>) => {
    setPending(true); setError('');
    try { await work(); }
    catch (e) { setError(String(e)); }
    finally { setPending(false); await Promise.all([check(), refresh()]); }
  };

  const help = (topic: string) => void invoke('open_setup_help', { topic }).catch(e => setError(String(e)));
  const prepare = () => void run(async () => {
    await invoke('run_setup', { action: 'prepare' });
    const status = await invoke<SetupStatus>('setup_status');
    setSetup(status);
    if (status.dockerReady && !status.restartRequired) await invoke('run_setup', { action: 'images' });
  });
  const save = (event: FormEvent) => {
    event.preventDefault();
    void run(async () => {
      await invoke('save_credentials', { user: username, password });
      setUsername(''); setPassword(''); setChangeCredentials(false);
    });
  };
  const leave = async () => {
    try { await invoke('dismiss_setup'); onLeave(verified ? workspaceId : undefined); }
    catch (e) { setError(String(e)); }
  };
  const details = activeWorkspace?.detail ?? (setup.busy ? setup.detail : pending ? 'Checking your setup…' : '');
  const problem = error || setup.error;
  const percent = setup.total ? Math.min(100, Math.round(setup.downloaded / setup.total * 100)) : undefined;

  return (
    <div className="flex h-svh flex-col bg-background text-foreground">
      <header className="flex shrink-0 items-center justify-between gap-4 border-b px-6 py-4">
        <div className="flex items-center gap-2.5"><Boxes className="size-5" /><span className="font-semibold">RegionBox</span><Badge variant="secondary">Setup</Badge></div>
        <Button variant="ghost" size="sm" disabled={busy} onClick={() => void leave()}>Set up later</Button>
      </header>
      <main className="flex-1 overflow-auto px-6 py-10 sm:py-14">
        <div className="mx-auto max-w-4xl">
          <h1 className="text-2xl font-semibold tracking-tight">Set up your regional browsers</h1>
          <p className="mt-3 max-w-xl text-sm leading-6 text-muted-foreground">RegionBox prepares this PC, then opens your first browser. Existing installations are reused. Downloads can take several minutes.</p>
          <div className="mt-8 grid items-start gap-8 md:grid-cols-[200px_1fr]">
            <nav aria-label="Setup progress">
              <ol className="space-y-2">
                {steps.map((label, index) => (
                  <li key={label} aria-current={step === index ? 'step' : undefined} className="flex items-center gap-3 py-2 text-sm">
                    <Badge variant={ready[index] || step === index ? 'default' : 'outline'} className="size-6 shrink-0 justify-center p-0">
                      {ready[index] ? <Check className="size-3.5" aria-hidden="true" /> : index + 1}
                    </Badge>
                    <span className={step === index ? 'font-medium' : 'text-muted-foreground'}>{label}<span className="sr-only">{ready[index] ? ', ready' : step === index ? ', current step' : ', pending'}</span></span>
                  </li>
                ))}
              </ol>
            </nav>
            <div className="min-w-0 space-y-4">
              {problem && <Alert variant="destructive"><CircleAlert /><AlertTitle>Setup needs attention</AlertTitle><AlertDescription><p className="break-words whitespace-pre-wrap">{problem}</p><p>You can retry this step. Completed steps are kept.</p></AlertDescription></Alert>}
              <Card>
                <CardHeader>
                  <CardDescription>{complete ? 'Setup complete' : `Step ${step + 1} of ${steps.length}`}</CardDescription>
                  <CardTitle className="text-xl">{complete ? 'Your first browser is ready' : setup.restartRequired ? 'Restart Windows to continue' : ['Prepare Windows support', 'Get Docker ready', 'Download browser files', 'Connect your NordVPN account', 'Open your first browser'][step]}</CardTitle>
                </CardHeader>
                <CardContent className="space-y-5">
                  {step <= 1 && <>
                    <p className="text-sm leading-6 text-muted-foreground">{setup.restartRequired ? 'Windows support has been installed. Restart your PC, then reopen RegionBox. Setup will continue from here.' : 'Docker runs each browser separately on this PC. RegionBox can install the required Windows support and Docker Desktop, then download the browser files.'}</p>
                    {!setup.supported && <Alert><CircleAlert /><AlertTitle>Check Windows compatibility</AlertTitle><AlertDescription>This build requires a supported 64-bit Windows 10 or 11 PC with an Intel or AMD processor.</AlertDescription></Alert>}
                    {setup.supported && !setup.virtualization && <Alert><CircleAlert /><AlertTitle>Turn on virtualization</AlertTitle><AlertDescription>Enable virtualization in your PC’s BIOS or UEFI, then restart Windows. The Windows help page explains the prerequisites.</AlertDescription></Alert>}
                    {!setup.restartRequired && <p className="text-sm leading-6 text-muted-foreground">Windows may ask for administrator approval or a restart. Complete any first-run prompts in Docker Desktop when it opens.</p>}
                    <div className="flex flex-wrap gap-2">
                      {setup.restartRequired ? <Button variant="outline" disabled={checking || busy} onClick={() => void check()}><RefreshCw />Check again</Button> : <Button disabled={busy || !setup.supported || !setup.virtualization} onClick={prepare}>{busy ? <LoaderCircle className="animate-spin motion-reduce:animate-none" /> : null}{setup.dockerInstalled && setup.windowsReady ? 'Start Docker and continue' : 'Set up this PC'}</Button>}
                      <Button variant="outline" onClick={() => help(step === 0 ? 'windows' : 'docker')}><ExternalLink />{step === 0 ? 'Windows setup help' : 'Docker setup help'}</Button>
                    </div>
                  </>}
                  {step === 2 && <>
                    <p className="text-sm leading-6 text-muted-foreground">Download Chromium and the VPN connection files once. Your workspaces will reuse these files. Allow several gigabytes of free disk space.</p>
                    <p className="text-sm">{setup.vpnImageReady ? 'VPN files are ready.' : 'VPN files are needed.'} {setup.browserImageReady ? 'Chromium is ready.' : 'Chromium files are needed.'}</p>
                    <Button disabled={busy} onClick={() => void run(() => invoke('run_setup', { action: 'images' }))}>{busy ? <LoaderCircle className="animate-spin motion-reduce:animate-none" /> : null}Download browser files</Button>
                  </>}
                  {step === 3 && <>
                    <NordCredentialsGuide onOpen={() => help('nord')} />
                    <form onSubmit={save} className="space-y-4">
                      <div className="space-y-2"><Label htmlFor="setup-user">Service username</Label><Input id="setup-user" autoComplete="off" spellCheck={false} value={username} onChange={e => setUsername(e.target.value)} placeholder="Your NordVPN service username" required /></div>
                      <div className="space-y-2"><Label htmlFor="setup-password">Service password</Label><Input id="setup-password" type="password" autoComplete="new-password" value={password} onChange={e => setPassword(e.target.value)} placeholder="Your NordVPN service password" required /></div>
                      <Button type="submit" disabled={busy || !username.trim() || !password.trim()}>Save and continue</Button>
                    </form>
                  </>}
                  {step === 4 && <>
                    {verified ? <>
                      <p className="text-sm leading-6">{selected.name} is connected. Its detected location is {countries[selected.network!.country]}.</p>
                      <p className="text-sm leading-6 text-muted-foreground">Open the browser inside RegionBox. You can start more workspaces from the sidebar and keep them running together.</p>
                      <Button onClick={() => void leave()} disabled={busy}>Open browser</Button>
                    </> : <>
                      <p className="text-sm leading-6 text-muted-foreground">Choose a country. RegionBox will connect NordVPN, start Chromium, and check the browser’s public IP.</p>
                      <div className="space-y-2"><Label htmlFor="first-workspace">First workspace</Label><NativeSelect id="first-workspace" value={workspaceId} onChange={e => setWorkspaceId(e.target.value)} disabled={busy}>{data.workspaces.map(workspace => <NativeSelectOption key={workspace.id} value={workspace.id}>{workspace.name}</NativeSelectOption>)}</NativeSelect></div>
                      {connected && <Alert><CircleAlert /><AlertTitle>Browser is running; location needs checking</AlertTitle><AlertDescription>{selected?.network ? `The IP check reported ${selected.network.country}. Your chosen country is ${selected.country}. Restart this workspace or check again.` : 'The location check did not complete. Retry Check IP before continuing.'}</AlertDescription></Alert>}
                      <div className="flex flex-wrap gap-2">
                        <Button disabled={busy} onClick={() => void run(() => invoke('start_workspace', { id: workspaceId }))}>{busy ? <LoaderCircle className="animate-spin motion-reduce:animate-none" /> : null}{connected ? 'Restart workspace' : 'Start first browser'}</Button>
                        {connected && <Button variant="outline" disabled={busy} onClick={() => void run(() => invoke('verify_workspace', { id: workspaceId }))}>Check IP</Button>}
                        <Button variant="ghost" disabled={busy} onClick={() => setChangeCredentials(true)}>Edit credentials</Button>
                      </div>
                    </>}
                  </>}
                  {busy && <div role="status" className="space-y-3 border-t pt-4">
                    <p className="flex items-start gap-2 text-sm leading-6"><LoaderCircle className="mt-1 size-4 shrink-0 animate-spin motion-reduce:animate-none" />{details}</p>
                    {setup.phase === 'downloading_docker' && percent !== undefined && <><Progress value={percent} aria-label="Docker download" /><p className="text-xs text-muted-foreground">{percent}% downloaded</p></>}
                  </div>}
                </CardContent>
              </Card>
              <div className="flex flex-wrap items-center justify-between gap-3">
                <p className="text-xs leading-5 text-muted-foreground">Setup progress is saved on this PC.</p>
                <Button variant="ghost" size="sm" disabled={busy || checking} onClick={() => void Promise.all([check(), refresh()])}><RefreshCw />Recheck setup</Button>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}
