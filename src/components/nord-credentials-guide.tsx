import { ExternalLink } from 'lucide-react';
import { Button } from '@/components/ui/button';

export function NordCredentialsGuide({ onOpen }: { onOpen: () => void }) {
  return (
    <div className="space-y-3">
      <p className="text-sm leading-6 text-muted-foreground">Use your NordVPN service credentials. These are different from your account email and login password.</p>
      <ol className="list-decimal space-y-1 pl-5 text-sm leading-6 text-muted-foreground">
        <li>In Nord Account, open NordVPN → Set up NordVPN manually.</li>
        <li>Verify your email if prompted.</li>
        <li>Under Service credentials, copy both the username and password into the fields below.</li>
      </ol>
      <Button type="button" size="sm" variant="outline" onClick={onOpen}><ExternalLink />Open Nord Account</Button>
    </div>
  );
}
