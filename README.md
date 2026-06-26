<p align="center">
  <img src="src-tauri/icons/source.svg" alt="Fieldnotes" width="120" />
</p>

# Fieldnotes

**A Holochain desktop app for structured scenario testing — run a shared script of test scenarios, record what you find, and have every finding live on a peer-to-peer network owned by the people doing the testing.**

Fieldnotes turns a test plan into a living, shared workspace. A team loads a campaign of scenarios (step-by-step things to try), and each tester works through them on their own machine — recording a verdict (Pass / Fail / Partial / Skip), writing free-text findings, and corroborating what others have already hit. The scenarios, verdicts, and findings all live on a Holochain DHT, replicated across everyone running the app. There's no central server holding the results.

It was built for a real job: testing the [Requests & Offers](https://github.com/happenings-community/requests-and-offers) (R&O) mutual-aid platform, by running its test scenarios through a tool that is itself a Holochain app. And it was built to make a point — that you don't have to be a developer to build a useful Holochain application. (See [Lineage](#lineage-proof-that-anyone-can-build-this).)

> **Alpha software.** This is Fieldnotes v0.1.2, an early release for invited testers. Expect rough edges. It is distributed as an unsigned build (see [Installing](#installing) for the workarounds).

---

## What Fieldnotes does

- **Shared scenario boards.** A campaign of test scenarios, grouped into sections, that every tester sees. Scenarios are imported in bulk and gated so only administrators can add them.
- **Verdicts and findings.** For each scenario, a tester records a verdict and adds findings — observations, steps to reproduce, anything worth keeping. Findings are an append-only thread; nothing is silently overwritten.
- **Corroboration.** A one-tap "same here" stamps a finding with the tester's environment (OS and architecture), so a pattern across machines becomes visible.
- **Encrypted attachments.** A tester can attach an image (a screenshot, a log capture) to their own finding, encrypted so only the administrator cohort can read it. Peers store the ciphertext but cannot read it. (New crypto — see the caveat under [What's New](#whats-new).)
- **Emergent-issue reports.** Testers can file issues that no scenario covered, with a live duplicate-check that surfaces similar existing reports as they type.
- **Verified identity.** Every tester links a [Flowsta Vault](https://flowsta.com/vault/) identity, so contributions resolve to a real person across devices, with no email harvesting or account farming.
- **Your data is yours.** A one-click export hands you a complete, portable copy of everything you've authored — a requirement of the licence Fieldnotes ships under, not an afterthought.
- **A nudge before you leak.** As you type a finding or report, Fieldnotes flags text that looks like private data (an email, a token, an IP) so you can review before submitting. Findings are public by design, so the help is in not putting private things in them by accident.

---

## What's New

### v0.1.2 — cross-platform alpha (June 2026)

The first invited-tester release.

- Scenario testing on a Holochain DHT: shared boards, per-tester verdicts, append-only findings, cross-machine corroboration, and emergent-issue reports with live duplicate detection.
- A validation-enforced administrator gate: scenario creation is restricted to administrators in the integrity layer, not just the UI. Administrator authority is held by the network's progenitor — set when you create the network from your Flowsta identity — and verified cryptographically, so a grant cannot be forged.
- Encrypted attachments scoped to the administrator cohort: a tester attaches an image to a finding, readable only by administrators. Adding an administrator later grants access without re-encrypting the image.
- Archive (not delete) for scenarios — hiding a scenario preserves every verdict and finding attached to it.
- A one-click, portable CAL-compliant export of everything you've authored.

> **Verified across agents.** The progenitor enforcement and peer-to-peer sync are covered by an automated multi-agent test: two separate agents on one network, where the progenitor can administer, a second agent's attempt to self-grant is rejected, and a scenario created by one syncs to the other.

> **Honest note on the encryption.** The encrypted-attachment crypto is new in this release. It has unit tests covering round-trip correctness, tamper detection, and wrong-key rejection, and it has been verified end-to-end in the running app — but it has **not** been through an external security review or audit. Treat it as defence-in-depth for an alpha on test data, not as a guarantee for genuinely sensitive material. If something must never be exposed, don't put it in this alpha.

---

## Installing

Fieldnotes is a desktop app. Download the build for your platform from the [Releases page](https://github.com/happenings-community/Fieldnotes/releases).

You also need the **Flowsta Vault** desktop app installed and a Flowsta identity — Fieldnotes uses it to sign you in. Vault only needs to be running for the first sign-in; after that, Fieldnotes works on its own.

### macOS — unsigned build

This alpha is **not code-signed or notarised**, so macOS Gatekeeper will block it on first open with a warning that it "cannot be opened because the developer cannot be verified." This is expected for an unsigned alpha. To open it:

1. **Right-click** (or Control-click) the Fieldnotes app in Finder.
2. Choose **Open** from the menu.
3. In the dialog, click **Open** again.

You only need to do this once. (Double-clicking the first time will *not* give you the Open option — you must right-click.) If macOS still refuses, clear the quarantine flag from a terminal: `xattr -dr com.apple.quarantine /Applications/Fieldnotes.app`. Signed builds are planned for a later release.

### Windows — unsigned build

This alpha is **not code-signed**, so Windows SmartScreen will warn on first launch that it's an unrecognised app. This is expected. To run it:

1. When SmartScreen appears, click **More info**.
2. Click **Run anyway**.

You only need to do this once. Signed builds are planned for a later release.

### Linux

The `.deb`, `.rpm`, and `.AppImage` builds carry no signing prompt. For the AppImage, make it executable first (`chmod +x Fieldnotes_*.AppImage`) then run it.

### First run

1. Open Fieldnotes. It starts its own Holochain conductor in the background (give it a moment on first launch).
2. Go to **Identity** and sign in with Flowsta. This links your identity to the network.
3. The **Scenarios** board fills in as it syncs from the network. Work through the scenarios — record a verdict, add findings, corroborate what others have hit.
4. File anything that no scenario covered under **Report**.

### Seeding scenarios (for administrators)

If you are setting up a network and need to load the scenario set:

1. Become an administrator (the first administrator self-grants from the
   **Identity** screen), then **reload the app**. The admin tools will not
   appear until you reload — the grant itself succeeds, but the running UI
   only re-checks your admin status on a refresh.
2. Import the scenarios from the raw test-tracker file. The importer expects
   the original `templates.json` format (fields `stepId`, `testArea`,
   `stepAction`, `lookFor`) — paste or upload that file directly. It is
   **not** markdown and not a pre-converted file; the app maps the fields and
   applies the administrator gate internally.

---

## How it's built

Fieldnotes is a [Tauri](https://tauri.app) desktop app (Rust backend, web frontend) wrapping a [Holochain](https://www.holochain.org) conductor.

- **Frontend:** Qwik, TypeScript, Tailwind CSS
- **Backend:** Tauri v2 (Rust), Holochain 0.6.1 (hdi 0.7.0, hdk 0.6.0)
- **Identity:** Flowsta agent linking
- **Encryption:** lair (`crypto_box`) wraps the per-administrator attachment keys; `ring` (ChaCha20-Poly1305) encrypts the attachment images themselves

The data model is three DHT entry types: **Item** (a scenario), **Response** (a tester's verdict, one per tester per Item), and **Finding** (an append-only observation, which can carry the encrypted attachments). Scenario creation is gated by a progenitor-signed `AdminGrant` verified at validation time, so a grant cannot be forged. Attachments use a two-stage scheme — the image is encrypted once under a per-attachment key, and only that small key is wrapped to each administrator — which is why adding an administrator later never re-encrypts the image.

The deeper infrastructure (conductor lifecycle, Flowsta integration, DNA migration, the encrypted-data approach) is inherited from ProofPoll largely unchanged; see ProofPoll's documentation and [docs.flowsta.com](https://docs.flowsta.com) for it. Fuller Fieldnotes-specific architecture notes are planned as separate docs.

---

## Lineage: proof that anyone can build this

Fieldnotes is a fork of **[ProofPoll](https://github.com/WeAreFlowsta/ProofPoll)** by Flowsta — a verified-polling Holochain app, explicitly built to be forked. ProofPoll solved the genuinely hard parts of a desktop Holochain app (conductor lifecycle, Flowsta identity linking, DNA migration, encrypted private data on a public DHT) so that the next person doesn't have to. Fieldnotes is that fork: the polling substrate became a scenario-testing substrate.

Flowsta's claim for ProofPoll is that you can build a real, useful Holochain application on top of it **without being a developer**. Fieldnotes is the evidence. It was built by a product owner who does not know any programming language — "usefully ignorant," comfortable at a terminal but not a coder — working with an AI assistant, across a handful of focused sittings — the initial build over a couple of days, then a later session that rebuilt the app around self-sovereign network creation, hardened and internally audited the security model, completed a full identity rename, and shipped this cross-platform release (the git history bears this out).

The stronger evidence is what happened when it broke. An early build shipped with the administrator gate inert — built, but not actually enforcing. Rather than paper over it, the same non-developer-plus-AI pairing traced the fault down through five layers to its root cause (a raw-versus-serialized signature mismatch in the integrity zome), fixed it, and went further — rebuilding the app around self-sovereign network creation and proving the enforcement holds across separate agents with an automated test. Finding, fixing, and hardening a real cryptographic bug is a higher bar than getting lucky on the first try — and it was cleared without writing the code by hand.

The headline is not *who* built it. It is *who can*. If someone who can't write code can produce a usable peer-to-peer application on this foundation, the barrier everyone assumed stood between an idea and a shipped hApp is lower than it looks. A companion guide — *building on ProofPoll as a non-developer* — is planned to turn this proof into a path others can follow.

---

## Running your own network

Fieldnotes is self-sovereign: there is no central network, and no key baked into the build. The first time you run it, you choose your network.

- **Create your own.** Pick "Create my network" and your Flowsta Vault identity becomes that network's *progenitor* — its sole administrator. A fresh, isolated network is generated for you. Only you can issue valid administrator grants on it; this is enforced cryptographically at validation time, not just in the UI.
- **Join one you've been invited to.** A network's administrator can generate an invite string (`fieldnotes://join?...`) from their Admin screen and share it. Paste it into "Join a network" on first run, and you join *their* network as a member. Members cannot grant themselves administrator — the network's progenitor is fixed at creation.

Because the network seed and progenitor are part of the network's identity, every created network is cryptographically distinct: different networks cannot see each other's data, and a member of one cannot administer another. Building from source is only needed to modify Fieldnotes itself — running your own network needs nothing but the app.

---

## A note on the alpha's gates

Fieldnotes is honest about what it does and doesn't enforce yet:

- **Membership** is not cryptographically gated in this alpha. Joining a network requires its invite string (seed + progenitor), which you share only with the people you want as members — so access is limited by *who you give the invite to*, not by a cryptographic join membrane. Anyone who obtains an invite can use it; a Flowsta-keyed membrane that gates membership at the network boundary is future work.
- **Administration** is cryptographically enforced. When you create a network, your Flowsta progenitor key is burned into that network's identity; only the progenitor can issue valid administrator grants, verified at validation time. This is proven across separate agents by an automated test — a non-progenitor's self-grant is rejected.
- **Attachment encryption** protects attached images from non-administrators, but its crypto is new and unaudited (see [What's New](#whats-new)). Treat it as alpha-grade defence-in-depth, not a guarantee. Finding *text* is public by design and only advisory-checked for accidental private data.

These are deliberate choices for an early alpha on public test data, not oversights — and they map the hardening path toward R&O's heavier model.

---

## Licence

Fieldnotes is licensed under the **[Cryptographic Autonomy License version 1.0](LICENSE)** (CAL-1.0).

It derives from **ProofPoll**, which is licensed under the **MIT License** — preserved verbatim in [`LICENSE.MIT`](LICENSE.MIT). The MIT License permits this relicensing provided the original notice is retained, which it is. See [`NOTICE`](NOTICE) for the full attribution and the split of what is whose.

  - Portions derived from ProofPoll: © 2026 ProofPoll contributors (MIT)
  - Portions original to Fieldnotes: © 2026 happenings community (CAL-1.0)
