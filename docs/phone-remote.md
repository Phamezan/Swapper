# Phone remote

## Phone Remote Control over LAN

1. Connect the PC and phone to the same trusted private Wi-Fi or Ethernet network. In Swapper **Settings → Remote**, turn on **Remote Control** and leave the transport on **LAN**.
2. The first time, the Remote tab shows **Allow on private networks**. Click it and approve the Windows prompt, which names Swapper; it adds a Windows Firewall rule for Private networks only. Swapper never asks on its own, and it starts no PowerShell or command windows. Swapper only offers LAN access when Windows identifies the active network as **Private** and selects an active default-route Ethernet or Wi-Fi interface. It does not advertise an address on a Public network.
3. Press **Pair a phone** and scan the QR code with the phone. Pairing links work once and expire after five minutes. The phone stays paired, also after Swapper or the PC restarts, until you revoke it in the paired devices list or use **Advanced → Reset LAN access…** and confirm **Reset and unpair all**, which unpairs every phone and shows a fresh QR code.
4. The phone opens the existing remote page. League Client must be running for queue and champion-select controls to be available.

LAN uses HTTP and is meant for trusted private networks. Do not use it on public or untrusted Wi-Fi. No router configuration, IP entry, or pairing code is needed.

A paired phone is tied to the PC's local address. If that address changes (for example after a router restart), Swapper shows **Your phone needs to reconnect** with the new QR code; scan it once and remove the old entry. Swapper also advertises itself as `swapper.local`, and an iPhone can switch to that name from the phone page so it keeps working when the address changes. Android browsers can't open `.local` addresses.

## Phone Remote Control with Tailscale

1. Follow [Tailscale's quickstart](https://tailscale.com/docs/how-to/quickstart) to install Tailscale on the Windows PC and your phone. Sign both devices into the same tailnet. See Tailscale's [Windows installation guide](https://tailscale.com/docs/install/windows) if needed.
2. Enable [MagicDNS and HTTPS certificates](https://tailscale.com/docs/how-to/set-up-https-certificates) for your tailnet. Swapper uses [Tailscale Serve](https://tailscale.com/docs/features/tailscale-serve) to expose its local page to devices allowed by your tailnet policy. Swapper does not use Funnel or make the page public on the internet.
3. In Swapper **Settings**, turn on **Remote Control**, select **Tailscale**, and open the displayed link on your phone or scan its QR code. Keep Tailscale connected on both devices.
4. Open League Client on the PC. The phone page shows connection status and the queue timer. At ready check you can accept the match; during champion select you can prepick, ban, pick, and lock in when League allows those actions.

Anyone permitted by your tailnet's access rules to reach this Serve route can use the page; Swapper leaves access control to Tailscale on this transport. See Tailscale's [Serve access guidance](https://tailscale.com/docs/features/tailscale-serve) if you share a tailnet. Account switching itself remains in the Windows tray app.
