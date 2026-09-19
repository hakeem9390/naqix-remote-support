# MeshCentral deployment runbook

Track A of the plan: get unattended remote support into customer Windows PCs
working, on a dedicated host, in ~1-2 weeks.

## Before you start

- [ ] A **dedicated** VPS. Not the Naqix prod box (`<naqix-prod-host>`). This stack
      holds SYSTEM-level access to customer machines; its blast radius must not
      overlap the ERP.
- [ ] A DNS A/AAAA record for the public FQDN, pointed at that VPS.
- [ ] Docker + Compose, and nginx + certbot on the host.
- [ ] An EV/OV code signing certificate ordered. SmartScreen reputation accrues
      over wall-clock weeks and cannot be compressed later, so start the clock
      now even though you don't need the cert until you sign agent installers.

## Deploy

```bash
cp .env.example .env
cp config/config.json.example config/config.json
```

Then edit both:

1. `.env` - set `MESH_HOSTNAME` and generate `PG_PASSWORD` with
   `openssl rand -base64 32`.
2. `config/config.json` - set `settings.cert` and `domains."".title` to your
   hostname and name, paste the same Postgres password, and generate
   `sessionKey` with `openssl rand -hex 32`.
3. `nginx/meshcentral.conf` - replace `remote.naqix.example` throughout (four
   occurrences) and set the real Naqix origin in the `frame-ancestors` directive.

Issue the certificate, install the nginx site, then:

```bash
docker compose up -d
```

Create the first admin account immediately - registration is closed
(`newAccounts: false`), so the first account through the web UI becomes admin
and no one else can self-register. Enable TOTP on it before doing anything else.

## Per-customer setup

One **device group** per ERP tenant. Grant each technician access only to the
groups they support. Use *domains* instead only if a customer ever needs their
own admin identity.

Install agents via per-customer invite links (`agentInviteCodes: true`), not a
shared installer - that gives you an install trail per tenant.

## Exit criterion

On a real customer PC, not a VM:

- [ ] Connect unattended from Chrome on macOS with nobody logged in at the far end
- [ ] Transfer a file in both directions
- [ ] Have the user lock their screen mid-session; stream survives
- [ ] Trigger a UAC prompt; the fallback behaviour is comprehensible
- [ ] Reboot the machine; agent reconnects on its own
- [ ] The viewer deep-links correctly from inside a Naqix iframe

## Operational notes

- **Use Chrome or Edge to control.** Safari and Firefox have no
  `navigator.keyboard.lock()`, so `Cmd+W`/`Cmd+Q`/`Cmd+Tab` hit your browser
  instead of the remote PC.
- **Ctrl+Alt+Del** can never be captured by any browser. Use the toolbar button,
  which asks the agent to call `SendSAS`.
- **Expect AV/EDR attention.** MeshCentral is popular with attackers as
  persistence tooling, so your agent binaries will draw scrutiny. Sign them, and
  submit false-positive reports to Microsoft and the major vendors.
- **Known weakness:** capture is GDI + tile-based JPEG, not H.264. Fine for ERP
  forms and grids, visibly behind AnyDesk on a poor link. That gap is what
  Track B's spikes exist to close.
