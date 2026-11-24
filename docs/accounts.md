# Accounts

## Add and switch accounts

1. Open Swapper. Right-click its tray icon and choose **Add Account**, or open the flyout and choose **Add account**.
2. Sign in through the official Riot Client with **Stay signed in** selected. League does not need to be open. Swapper detects the Riot account, then offers **Save Account**. If that same account is already saved, it offers **Update Saved Account**.
3. Left-click the Swapper tray icon to open your accounts. Left-click an account to switch. Right-click an account for **Switch to account**, **Edit account**, **Remove account**, and **Add account**. Removing an account asks for confirmation.

Do not use Riot Client's **Sign out** between saved accounts; it can invalidate a session that Swapper needs to restore. Use Swapper's **Sign in to another account** flow. If a saved session expires, sign back into that same account in Riot Client and update its saved session. Accounts created by older Swapper builds without a stored account identifier cannot be verified automatically; save a new verified entry before removing the old one.

## Account features

- Save multiple Riot Client sign-ins and switch between them from the tray flyout. Swapper closes Riot and League client processes before restoring the selected session; it refuses to switch while a game is running.
- Detect the signed-in Riot account automatically and avoid duplicate entries using Riot's account identifier. Give saved accounts optional nicknames and remove them from the tray app.
- **Repair Account** when a saved session has expired or can't be restored: sign in again in Riot Client and Swapper replaces the saved session after checking it is the same account. After each switch Swapper watches for Riot's login screen and offers the repair when it appears.
- **Desktop shortcuts** that switch straight to one account (`swapper://switch/…` links), and an optional **global hotkey** that opens the flyout.
- Start Swapper when you sign in to Windows, and pin the tray flyout open while you use it. Optionally launch through the bundled [Deceive](https://github.com/molenzwiebel/Deceive) executable to use Deceive's presence behavior.
- Optionally show League's in-game interface in **Korean** with the bundled Korean font mod (off by default). It changes League's game files while a game runs, so use it at your own risk.
