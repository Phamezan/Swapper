# Security

## Reporting a vulnerability

Report security problems privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability** to file a private security advisory on [github.com/Phamezan/Swapper](https://github.com/Phamezan/Swapper). Do not open public issues for security problems, and do not include Riot account identifiers, session files or access tokens in a report.

## Privacy and storage

Swapper stores account names, Riot account identifiers, settings, and references to encrypted session snapshots in `%LOCALAPPDATA%\Swapper`. Session snapshots are encrypted with Windows DPAPI for the current Windows user and are not sent to the phone or a Swapper server. Riot's own live client files remain under `%LOCALAPPDATA%\Riot Games`. Copying a vault to a different Windows account generally will not make it usable there.

Swapper reads the local Riot and League clients to identify a signed-in account and provide remote controls. It does not request passwords or use a Riot developer API key. For public statistics, game data and updates it contacts op.gg, lolalytics, u.gg, Riot's Data Dragon, CommunityDragon and GitHub; the only player identifier it sends to any of them is the Riot ID and region of a profile you open, to u.gg, and only when your League Client didn't return that player's season record. Riot may change its clients independently, so a future client update can require a Swapper update.
