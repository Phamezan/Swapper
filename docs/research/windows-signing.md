# Windows code signing for Swapper releases

Research for the P0 TODO "Signed Windows releases" items:

- "Investigate free OSS certificate such as https://ossign.org/ or https://signpath.org/"
- "Verify whether bundled `Deceive.exe` affects signing requirements."

All sources were accessed on **2026-09-28**. "Primary" means the vendor's own
documentation; secondary sources are labelled as such. This document is research
only and does not change any source code or workflow. It intentionally does not
edit `.github/workflows/windows-installer.yml` (another worker owns it).

## TL;DR

- **SignPath Foundation** is the only free option with a documented, enforceable
  policy and a working GitHub Actions integration. Swapper meets the *hard*
  criteria (MIT, public repo, existing releases, CI builds on GitHub-hosted
  runners, documented download page) but the *soft* criterion is the risk:
  SignPath explicitly requires "a certain verifiable reputation" and the project
  is 3 days old with 0 stars. Expect a likely rejection until it has a real user
  base. Re-apply later.
- **OSSign is not currently a usable option.** Applications are suspended, and
  Swapper would fail its "absolute minimum of 6 months of activity" rule anyway.
  Its trust story is thin and unverifiable.
- **No code signing path removes the SmartScreen warning on a brand-new
  binary.** Since 2024 Microsoft treats OV and EV the same; reputation accrues
  per file hash and per publisher identity over time. Consistent signing is what
  eventually removes the prompt.
- **Bundled `Deceive.exe` does not need re-signing.** It is unsigned upstream
  (verified), and SignPath policy explicitly allows unsigned upstream OSS
  binaries inside a signed installer. Swapper must *not* sign Deceive itself.
- **Azure Artifact Signing** (formerly Azure Trusted Signing) is the realistic
  paid baseline at $9.99/month, but only for US/Canada individual developers or
  organizations in a specific country list.

---

## 1. SignPath Foundation (signpath.org)

### What it is

SignPath Foundation provides free code-signing certificates to open-source
projects, using SignPath.io to hold the key in an HSM. SignPath Foundation is
operated by SignPath GmbH (Vienna, Austria), the company behind SignPath.io
(<https://signpath.org/about>). The certificate is issued **to SignPath
Foundation**, not to the project, so the publisher shown to Windows users is
"SignPath Foundation" (<https://signpath.org/terms>).

### Eligibility criteria (primary: signpath.org/terms)

Conditions for a free OSS subscription:

- **No malware / no potentially unwanted programs.**
- **OSI-approved open-source license, without commercial dual-licensing for all
  components.**
- **No proprietary code** (except System Libraries).
- **Actively maintained.**
- **Already released** in the form to be signed.
- **Documented** on its download page or app-store entry.

Additional conditions when the certificate comes from SignPath Foundation:

- **Sign your own projects/binaries only.** No signing upstream third-party
  binaries. You *may* include unsigned upstream OSS binaries inside a signed
  package (e.g. an installer).
- **Modified upstream may be signed** only if the upstream publishes signed
  builds, the fork is visible, release branches are based on upstream signed
  branches, and code review obligations are met.
- **No hacking tools**: no features that exploit vulnerabilities or circumvent
  the security measures of the execution environment. (Not a prohibition on
  security diagnostics or on ordinary game/client tooling.)
- **Respect privacy and security; announce system changes; provide
  uninstallation.**
- **Contributor rules:** MFA on both GitHub and SignPath; defined
  Authors/Reviewers/Approvers; every signing request approved by an Approver.
- **Website/repo rules:** the project home page must have a section literally
  titled "Code signing policy" with the wording
  "Free code signing provided by SignPath.io, certificate by SignPath
  Foundation", the team roles, and a privacy-policy link.
- **Artifact configuration rules:** product name/version metadata must be set as
  enforced metadata restrictions on all signed binaries.
- **Per-release manual approval** and **verifiable builds from source** are
  non-negotiable ("Do not fight the system").

### Does Swapper plausibly qualify?

Hard criteria — yes:

| Criterion | Swapper | Status |
| --- | --- | --- |
| OSI license, no dual license | MIT (`LICENSE`) | Pass |
| Public repo | `github.com/Phamezan/Swapper` (via `gh api`) | Pass |
| Already released | v0.2.0 (2026-09-25), v0.3.0 (2026-09-27) | Pass |
| Documented download page | README has Download/Features sections | Pass |
| CI build on GitHub-hosted runner | `windows-latest` in `windows-installer.yml` | Pass |
| No proprietary code | All Swapper code MIT; Deceive is separate GPL OSS | Pass with disclosure |
| MFA + Code signing policy page | Not yet set up | Owner action |
| Actively maintained | 3 days old, 0 stars, 1 open issue | **Risk** |
| Verifiable reputation | None yet | **Risk** |

Precedent in the same category: **TcNo Account Switcher** (an account switcher)
and **Pengu Loader** (a League of Legends client tool) are both listed SignPath
Foundation projects (<https://signpath.org/projects>), so the problem domain is
not automatically disqualifying. The "No hacking tools" clause is about
exploiting vulnerabilities, not about game-client or presence tooling.

Honest assessment: **apply, but expect to be turned down on reputation the
first time.** SignPath's own FAQ states it cannot sign "binaries based on source
code that nobody knows" and requires more than a license check. Re-apply after
Swapper has releases, downloads, and users.

### Application process and turnaround

- Apply through the form at <https://signpath.org/apply> (an embedded HubSpot
  form).
- Install the SignPath GitHub App (<https://github.com/apps/signpath>) and
  enable MFA on GitHub and SignPath.
- SignPath then issues the organization ID, project slug, and signing policy
  slug and grants dashboard access.
- Turnaround is not stated on the primary page. Anecdotal secondary reports:
  ~1 week (<https://github.com/amd/gaia/issues/732>) and "a few days"
  (<https://rubentalstra.github.io/Trial-Submission-Studio/development/windows-signing.html>).
  Treat as weeks, not days, and expect questions about the Deceive bundle.

### Constraints that shape the pipeline

- The build must run through the predefined **GitHub.com** trusted build system,
  and for OSS projects **all jobs up to the signing request must run on
  GitHub-hosted agents** (<https://docs.signpath.io/trusted-build-systems/github>).
- The unsigned artifact must first be uploaded with `actions/upload-artifact`
  and passed to SignPath by `artifact-id`; SignPath verifies origin metadata
  from GitHub itself, not from the build script.
- **Origin verification** ties the signature to a specific repository/ref
  (<https://docs.signpath.io/origin-verification>), so signing should be
  restricted to release tags/branches.
- **Every release needs manual approval** in the SignPath dashboard.

### GitHub Action

`signpath/github-action-submit-signing-request` (current major version `@v3`),
documented at <https://docs.signpath.io/trusted-build-systems/github> and
<https://github.com/SignPath/github-action-submit-signing-request>. Required
inputs: `api-token`, `organization-id`, `project-slug`, `signing-policy-slug`,
`github-artifact-id`. Use `wait-for-completion: true` and
`output-artifact-directory` to get the signed file back. Because
`actions/upload-artifact` stores a ZIP by default, the artifact configuration's
root element must be `<zip-file>` unless `archive: false` is used.

---

## 2. OSSign (ossign.org)

### What it is

A free code-signing program for OSS projects, run from Uppsala, Sweden, that
also sells SSL/document/code-signing certificates
(<https://ossign.org/>, primary). Stated model: certificates "recognized
globally", "public chain of trust", EV "available (contact us)".

### Eligibility (primary: ossign.org)

- Majority of the project under an OSI-approved license; all release code
  publicly reviewable.
- Active project with a demonstrable user base or community.
- **Absolute minimum of 6 months of activity** on the account/organization/
  project (and for forks, on the upstream project too).
- A public automated build pipeline with code-quality checks, where the source
  for each build is directly linked to the release binaries.
- Commercial projects considered case-by-case.
- No political/controversial projects, including offensive-security tools.

### Current status

**Applications are suspended** "due to a high workload and large backlog while
we focus on assessing and responding to the applications in the queue ... check
back in a few weeks" (primary, banner on the application section of
<https://ossign.org/>). The site says the contact form is not accepting signing
applications either.

### Trust and SmartScreen implications — be skeptical

The information is thin. Specifically:

- No certificate authority is named, and no WebTrust/CA-audit report or root
  store inclusion is linked, so a publicly verifiable chain to the Microsoft
  trusted root cannot be confirmed from the site.
- No list of already-signed projects is published, and the FAQ is a JS
  accordion, so its answers are not publicly verifiable.
- No legal entity registration or team is disclosed beyond "Uppsala, Sweden".
- Nothing on the site states how their (OV or EV) certificate behaves with
  SmartScreen.

Verdict: treat OSSign as **not an option now** (suspended) and as **unproven**
even later. Swapper also fails the 6-month activity minimum regardless. Do not
rely on it; revisit only if SignPath and paid options are both unacceptable.

---

## 3. Comparison: SmartScreen behaviour, cost, effort

### SmartScreen reality (primary: Microsoft Learn)

Microsoft's current guidance, "SmartScreen reputation for Windows app
developers" (<https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation>,
accessed 2026-09-28):

- SmartScreen evaluates **publisher reputation** (is it signed? is the signer
  known?) *and* **file-hash reputation** (has this exact file been downloaded
  and run without incidents?).
- A valid signature does not buy silence. A newly created binary can still show
  "unrecognized app" until its hash or publisher certificate accumulates
  positive reputation. The verified publisher name *is* displayed.
- Unsigned files must rebuild reputation for every new version. Signed files can
  let certificate/publisher reputation carry across versions — which is the
  main practical reason to sign beyond "no scary warning".
- **"EV certificates no longer bypass SmartScreen"** — EV and OV are treated the
  same for SmartScreen. EV still matters for some enterprise procurement, not
  for the download prompt.
- Actions that help: publish to the Store, **sign every release**, do not modify
  files after signing, do not sign PUP-behaving software, and **use a consistent
  signing identity**.

SignPath's own knowledge base ("Windows Platform",
<https://signpath.io/knowledge-base/windows-platform>) agrees on the OV
reputation-building model and recommends EV for internet downloads — but that
page predates/does not reflect the 2024 change that EV no longer bypasses
SmartScreen. Trust the Microsoft page for the current behaviour.

### What each certificate means for Swapper

| Option | Cost | Signer identity shown | SmartScreen | Effort |
| --- | --- | --- | --- | --- |
| SignPath Foundation | Free | "SignPath Foundation" (shared, OV-class) | New binaries may warn until reputation builds; shared publisher identity may inherit some Foundation reputation | Low-to-medium: apply, sign, per-release approval |
| Azure Artifact Signing (Trusted Signing) Basic | $9.99/mo (~$120/yr) up to 5,000 signatures; Premium $99.99/mo | Your validated legal name/entity | Same as any valid cert: may warn on brand-new binaries; publisher identity is yours and consistent | Medium: Azure + identity validation + action config |
| Paid OV certificate (CA) | ~$70–300/yr | Your entity | Same warning behaviour; reputation does not inherit across renewals | Medium-high: identity verification, token/HSM handling |
| Paid EV certificate (CA) | ~$299+/yr | Your entity | **No longer an automatic bypass**; enterprise/whitelisting benefit only | High: hardware/HSM requirements |
| Self-signed / unsigned | Free | none / unknown publisher | Full "Windows protected your PC" prompt; rebuilds reputation every version | None |

Signing identity is the important long-run lever: a stable publisher identity
lets reputation carry to future releases. SignPath's shared "SignPath
Foundation" identity is stable across releases, but it is not Swapper-specific;
Azure's identity is Swapper-specific but costs money and has geographic
restrictions.

### Azure Artifact Signing as a paid baseline (secondary use only)

Renamed from **Azure Trusted Signing** to **Azure Artifact Signing** in 2026
(<https://learn.microsoft.com/en-us/azure/artifact-signing/overview>).

- Pricing: Basic $9.99/month (5,000 signatures), Premium $99.99/month (100,000),
  then $0.005/signature
  (<https://azure.microsoft.com/en-us/pricing/details/artifact-signing/>).
- Scope: Windows Authenticode (and private trust/driver scenarios); short-lived
  certificates renewed daily, timestamped.
- Eligibility: Public Trust is for organizations in the **US, Canada, EU, UK,
  Australia, New Zealand, Japan, South Korea, Singapore, Switzerland, Norway,
  Israel**; **individual developers must be located in the US or Canada**
  (primary: <https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart>).
  An individual validation also requires an Azure billing account with Account
  Type "Individual" matching the government ID.
- Identity validation takes 1–20 business days.
- Note: an OSS project can use Azure too — LocalSend signs Windows artifacts
  with Azure Trusted Signing/Artifact Signing in its release workflow
  (<https://github.com/localsend/localsend/blob/main/.github/workflows/build_windows_exe.yml>).

For Swapper, Azure only makes sense if the owner is a US/Canada individual or an
eligible-country organization willing to pay ~$120/yr. It is the fallback if
SignPath says no.

---

## 4. Bundled `Deceive.exe` and signing

### Current facts

- `src-tauri/tauri.conf.json` bundles `resources/Deceive.exe` plus its GPL-3.0
  license and source notice. `Deceive-SOURCE.txt` records Deceive **v1.18.0**,
  SHA-256 `25dc5427affed66aa38ec1d9103ffa1a47256dab4e698bef32543fbf0cf3d2e5`.
- The inclusion is documented in `README.md` ("Open-source credits and
  licenses").
- **Deceive.exe is not digitally signed.** Verified locally with
  `Get-AuthenticodeSignature src-tauri/resources/Deceive.exe` → `NotSigned`
  (2026-09-28). That file is the exact upstream v1.18.0 binary.
- Upstream `molenzwiebel/Deceive` releases publish only a single `Deceive.exe`
  asset with no signature or checksum asset (GitHub API, 2026-09-28), and the
  project is GPL-3.0.

### Does signing Swapper require or imply re-signing Deceive?

**No, and Swapper must not re-sign it.**

- SignPath policy explicitly permits unsigned upstream OSS binaries inside a
  signed package: "You may include unsigned binaries of upstream OSS projects,
  e.g. DLL files, in your signed packages, e.g. MSI installers"
  (<https://signpath.org/terms>). It expresses a preference for getting upstream
  to sign, but does not require it. LocalSend's public policy states the same:
  "Third-party binaries that are packaged with the app ... do not receive the
  [project's] signing operation"
  (<https://github.com/localsend/localsend/blob/main/CODE_SIGNING.md>).
- Signing Deceive with Swapper's SignPath certificate would violate the "sign
  your own binaries only" rule. The fork exception does **not** apply: Deceive
  upstream does not publish signed builds.
- Re-signing an unmodified third-party binary would also misrepresent authorship
  and is unnecessary.
- Optional improvement: ask Deceive upstream to get its own signing (SignPath or
  otherwise). Until then, distribute Deceive unsigned, as today.

### Practical Windows/SmartScreen effect

- The NSIS installer is what users download; it carries Mark-of-the-Web. Files
  the installer *extracts* (including `Deceive.exe`) are not normally marked
  with MotW, so `Deceive.exe` usually will not independently trigger the
  SmartScreen download prompt when Swapper launches it.
- Signing the installer does not make `Deceive.exe` signed, and Defender/AV can
  still flag a presence-spoofing game tool independently of code signing. That
  is outside the signing pipeline's control.

### Licensing implications

- Deceive is GPL-3.0. Distributing the unmodified executable requires conveying
  the corresponding source and the license. Swapper already ships
  `Deceive-LICENSE.txt`, `Deceive-SOURCE.txt`, and a
  `Deceive-v1.18.0-source.zip` release asset, so that obligation is met.
- Signing does not change GPL obligations, but if Swapper ever *modified*
  Deceive it would have to publish the modified source (and the SignPath fork
  conditions would also apply). Keep it unmodified.
- Policy risk, not licensing: SignPath's "No malware / no potentially unwanted
  programs" and "No hacking tools" clauses leave a discretionary opening. The
  account-switcher category is already represented (TcNo), but disclose the
  Deceive bundle explicitly in the application so it cannot be seen as
  concealed.

---

## 5. Recommended pipeline for Swapper

### What changes in `.github/workflows/windows-installer.yml`

Today the workflow is `workflow_dispatch` only; it builds the NSIS installer and
uploads the **unsigned** `*.exe` as a workflow artifact
(lines 44–50). To sign, keep the build, then:

1. upload the unsigned installer with `actions/upload-artifact` (needed for the
   SignPath `artifact-id`),
2. submit a SignPath signing request,
3. take SignPath's signed output directory and upload that as the release
   artifact instead of the unsigned build output.

Pseudo-YAML (illustrative; do **not** apply here — the workflow file is owned by
another worker):

```yaml
# ... existing checkout / Deceive hash check / toolchain / npm ci ...
- run: npm run tauri build -- --target x86_64-pc-windows-msvc --bundles nsis

# Deep signing: sign Swapper's own binaries before they are packaged, if the
# artifact configuration is set up for it. Otherwise skip and sign only the
# installer (simpler first step).
# - name: deep-sign app binaries
#   uses: signpath/github-action-submit-signing-request@v3
#   with:
#     api-token: ${{ secrets.SIGNPATH_API_TOKEN }}
#     organization-id: ${{ vars.SIGNPATH_ORG_ID }}
#     project-slug: swapper
#     signing-policy-slug: release-signing
#     artifact-configuration-slug: swapper-unsigned-binaries
#     github-artifact-id: ${{ steps.upload-unsigned-binaries.outputs.artifact-id }}
#     output-artifact-directory: src-tauri/target/x86_64-pc-windows-msvc/release

- name: upload unsigned installer
  id: upload-unsigned-installer
  uses: actions/upload-artifact@v4
  with:
    name: swapper-unsigned-installer
    path: src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe
    if-no-files-found: error

- name: sign installer
  uses: signpath/github-action-submit-signing-request@v3
  with:
    api-token: ${{ secrets.SIGNPATH_API_TOKEN }}
    organization-id: ${{ vars.SIGNPATH_ORG_ID }}
    project-slug: swapper
    signing-policy-slug: release-signing
    artifact-configuration-slug: swapper-installer   # root <zip-file>, unless archive:false
    github-artifact-id: ${{ steps.upload-unsigned-installer.outputs.artifact-id }}
    wait-for-completion: true
    output-artifact-directory: signed

- uses: actions/upload-artifact@v4
  with:
    name: Swapper-Windows-signed
    path: signed/**/*.exe
    if-no-files-found: error
```

Notes on the exact shape:

- The current job only uploads an artifact; a real release also needs a
  tag-triggered `release` job (or a `softprops/action-gh-release` /
  `gh release upload` step) that publishes the **signed** file. Restrict the
  SignPath policy to release tags for origin verification.
- `output-artifact-directory` receives the signed file(s) extracted from the
  SignPath ZIP. Point the final `upload-artifact`/release step at that
  directory.
- `upload-artifact` produces a ZIP, so the SignPath artifact configuration root
  must be `<zip-file>` unless `archive: false` is set.
- Deceive is intentionally excluded from signing; the artifact configuration
  should target only Swapper's own binaries and the installer.
- All jobs already run on `windows-latest`, which satisfies SignPath's
  GitHub-hosted-runner requirement.

### What the owner must do manually

1. Apply at <https://signpath.org/apply>, disclosing the Deceive bundle and the
   account-switching purpose. Expect reputation questions.
2. Enable MFA on GitHub and SignPath; install the SignPath GitHub App on
   `Phamezan/Swapper`.
3. Define team roles (Authors/Reviewers/Approvers) and add a "Code signing
   policy" section to `README.md` with the exact required wording, roles, and
   privacy-policy link (SignPath requires this before/at signing).
4. In SignPath, create the project and a `release-signing` policy restricted to
   release tags, with at least one Approver; create the artifact configurations
   for the installer (and optionally deep-signed binaries).
5. Add repository secret `SIGNPATH_API_TOKEN` and repository variable
   `SIGNPATH_ORG_ID` (or inline the IDs).
6. On each release, approve the signing request in the SignPath dashboard.
7. If SignPath declines, evaluate Azure Artifact Signing (see §3) as the paid
   fallback.

### TODO items this answers

- "Investigate free OSS certificate such as signpath.org or ossign.org" —
  answered in §1 and §2.
- "Verify whether bundled Deceive.exe affects signing requirements" — answered
  in §4: it does not require re-signing; include it unsigned under SignPath's
  explicit allowance.

---

## Sources

Primary (vendor documentation):

- <https://signpath.org/> — free OSS signing overview (accessed 2026-09-28)
- <https://signpath.org/about> — operator is SignPath GmbH (accessed 2026-09-28)
- <https://signpath.org/terms> — eligibility and certificate conditions (accessed 2026-09-28)
- <https://signpath.org/apply> — application form (accessed 2026-09-28)
- <https://signpath.org/projects> — accepted projects incl. TcNo, Pengu Loader (accessed 2026-09-28)
- <https://docs.signpath.io/trusted-build-systems/github> — GitHub checks, action inputs, policies (accessed 2026-09-28)
- <https://github.com/SignPath/github-action-submit-signing-request> — action (accessed 2026-09-28)
- <https://docs.signpath.io/origin-verification> — origin verification (accessed 2026-09-28)
- <https://signpath.io/knowledge-base/windows-platform> — SmartScreen/OV/EV background (accessed 2026-09-28)
- <https://ossign.org/> — program, criteria, suspended-applications notice (accessed 2026-09-28)
- <https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation> — SmartScreen reputation (accessed 2026-09-28)
- <https://learn.microsoft.com/en-us/azure/artifact-signing/overview> — Azure Artifact Signing (accessed 2026-09-28)
- <https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart> — eligibility, identity validation (accessed 2026-09-28)
- <https://azure.microsoft.com/en-us/pricing/details/artifact-signing/> — pricing (accessed 2026-09-28)
- <https://github.com/localsend/localsend/blob/main/CODE_SIGNING.md> — OSS signing policy and third-party binary handling (accessed 2026-09-28)
- <https://github.com/localsend/localsend/blob/main/.github/workflows/build_windows_exe.yml> — worked Azure signing example (accessed 2026-09-28)
- <https://github.com/molenzwiebel/Deceive/releases> — Deceive v1.18.0, GPL-3.0, unsigned (accessed 2026-09-28)
- <https://github.com/Phamezan/Swapper> and `gh api repos/Phamezan/Swapper` — MIT, public, releases/age (accessed 2026-09-28)

Secondary (anecdotal; used only for turnaround estimates):

- <https://github.com/amd/gaia/issues/732> — "~1 week" SignPath approval
- <https://rubentalstra.github.io/Trial-Submission-Studio/development/windows-signing.html> — signpath.org application walk-through ("a few days")

Local verification:

- `Get-AuthenticodeSignature src-tauri/resources/Deceive.exe` → `NotSigned` (2026-09-28).
