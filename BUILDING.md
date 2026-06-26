# Building on ProofPoll as a non-developer

This is a companion to Fieldnotes: a guide to building your own Holochain app by
forking [ProofPoll](https://github.com/WeAreFlowsta/ProofPoll), the way Fieldnotes
was built — by someone who does not write code, working with an AI assistant.

It has three parts:

1. **The method** — how the human-and-AI working relationship has to work for this
   to produce something real and safe, rather than something that merely compiles.
2. **The practical path** — the actual steps, from fork to released alpha.
3. **A payload to give your AI** — a copy-paste block that sets your AI assistant
   up to work in this way from the first message.

The honest claim is narrow and worth stating plainly: you do not need to know a
programming language to build a useful Holochain app on this foundation. You *do*
need product judgement, a refusal to accept assertions without evidence, and the
discipline described below. The AI does the reading, the design, and the code. You
hold the line on what you're building, why, and whether it's actually right.

---

## Is this for you?

This guide is for you if you have a clear idea of an app you want, you're willing to
work patiently and methodically, and you're comfortable enough at a terminal to run
commands someone gives you and report what comes back. You do **not** need to be a
programmer, or to understand the code.

It is probably *not* for you if you want to type one sentence — "build me a booking
app" — and have a finished product appear. That doesn't work, and the rest of this
guide is largely about why. What does work is slower, more involved, and more
rewarding: you and the AI build the thing together, in a deliberate sequence, one
verified step at a time.

### You need the concepts, not the code — and here's where to get them

Let's be honest about the boundary, because it matters. You do **not** need to be
able to read or write code, and you do not need to memorise jargon before you start —
your AI knows the code and the terms, and you can ask it to explain any of them in
plain language at any time. ("Explain that to me as if I don't code" is a completely
legitimate request you should make often; slowing down to understand is the work, not
a distraction from it.)

But there's a real floor, and it's worth being straight about: you *do* need a
working grasp of the **ideas** this is built on. You can't hand this guide and a
laptop to someone with no feel for what peer-to-peer means and expect an app at the
end. The whole method below — especially catching when the AI reaches for the wrong,
centralised shape (principle 7) — depends on you understanding *concepts* like: there
is no central server or database; every participant runs their own copy; data is
shared and validated across peers; identity is something a person holds, not an
account a company issues. You don't need to know how those are implemented. You do
need to understand *that they're true*, and roughly why, so you can make the
judgement calls that are yours to make.

The good news: that conceptual grounding is freely available and readable without
being a developer. Before and during your build, lean on:

- **[Holochain's Developer Portal](https://developer.holochain.org/)** — start with
  its **Core Concepts** and the **Glossary** for plain-language explanations of the
  ideas (agent-centric, the DHT, validation) without needing to code.
- **[The Holochain blog](https://blog.holochain.org/)** — context, examples, and the
  thinking behind the design.
- **[Sacha's Holochain agent skill](https://github.com/Soushi888/holochain-agent-skill)**
  (below) — primarily for your AI, but its framing of the development "spiral" is
  itself a useful map.
- **[Our own hAppenings newsletters](https://happeningscommunity.substack.com/)** —
  where this build and others are written up for non-developers, deliberately in
  plain language.

A weekend getting comfortable with the *ideas* — not the code — is the single best
preparation. The rest you learn by asking your AI as you go.

### Be methodical: build a sequence and stick to it

The biggest mistake is treating the AI like a vending machine — one big request in,
a finished app out. It will fail, because a real app is a hundred connected
decisions, and made all at once they collapse into a confident mess.

The approach that works is the opposite:

- **Break the build into steps.** Before writing anything, ask the AI to help you
  lay out a logical sequence: what gets built first, what depends on what, what order
  makes sense. Agree the plan together.
- **Do one step at a time.** Build it, check it actually works (see principle 5),
  then move to the next. Don't let several half-finished things pile up.
- **Stick to the sequence** — but revise it openly when you learn something. A plan
  you adjust deliberately is fine; drifting from it by accident is not.

This is slower than "make me an app," and that slowness is exactly what produces
something that works. You are the one holding the plan; the AI executes against it.

---

## Part 1 — The method

The story is not "an AI wrote an app." It is "a non-developer and an AI worked with
discipline and shipped something real." The difference between those two is entirely
the method. Seven principles carried the whole build, and the same seven will carry
yours.

### 1. Source before reasoning — always

The single most important rule. Before your AI reasons about any code, it reads the
*actual* file or the *actual* error message — it does not reason from memory about
what the code "probably" says.

Here is the thing to understand about why. The AI is astonishingly capable and,
intermittently, confidently wrong — and the two are not always separable. It is the
brilliant colleague who has read every book in the library and never once set foot in
the lab. Ask it the general shape of a thing and it's usually right. Ask it the exact
name of an unusual function, or the precise behaviour of a tool it has never actually
run, and it may invent an answer with total conviction. You often cannot tell which
kind of answer you're getting — so the whole discipline reduces to one habit: *never
trust the claim, check the source.*

The recurring failure mode is an AI confidently describing code it hasn't looked at.
The fix is the habit above: *show me the real file; show me the real error.* You
can't eyeball-verify the code yourself — so this is how you stay safe. Over and over
in the Fieldnotes build, reading the real thing caught problems that
reasoning-from-memory would have shipped: a hard size limit in the encryption layer
that an assumption got wrong twice; a dead code path that would have errored in front
of testers; a data-corruption hazard hiding in an enum.

You enforce this by asking, every time something is uncertain: *have you actually
read that, or are you guessing?*

There's a structural reason this works so well in practice, and it's worth seeing as
a strength rather than an inconvenience: your AI runs *separately* from your code. It
can't reach into your files and change them directly — every edit, every check, every
look at the real state has to pass through *you* running a command and reporting what
came back. That separation is an air-gap, and it's exactly what enforces the
discipline. It makes it impossible for the AI to quietly "just do it" by reaching
past you into the code — the same reach-for-the-centre habit you're guarding against
elsewhere. The human in the loop isn't a bottleneck here; it's the safety mechanism.
When the AI genuinely can't see something — a file, the current state, a page on the
web — the right move is for it to *say so and ask you to fetch it*, never to fill the
gap from memory or assume the thing is simply unavailable.

### 2. Guarded, reversible edits

Your AI should never change a file blind. The pattern that works: a small script
that anchors on an exact, unique piece of text, refuses to run if that anchor isn't
found exactly once, makes the change to a temporary copy first, and prints the
difference for you to look at *before* anything real is touched.

This is how a non-coder safely edits code they can't fully read. Every change is
previewed and reversible. If the AI proposes editing a file by just "rewriting" it,
slow down — ask for the guarded, diff-first version instead.

### 3. You hold the line

Your judgement overrides the AI's momentum, and the AI should push back on you too.
This two-way honesty — not deference in either direction — is what keeps the work
clean. In the Fieldnotes build the human refused, more than once, to ship something
that was technically working but not *right*; the AI, in turn, refused to commit
files it hadn't actually read and flagged when a big change should wait for a fresh
head. Both were correct to hold their ground.

If your AI is agreeing with everything you say, that is a problem, not a comfort.
Ask it where it disagrees.

Holding the line is not only about individual decisions — it's also about the shape
of a long session. Over a long stretch an AI can start to overcomplicate, over-offer
options, or quietly lose the thread of what you're actually trying to do. You are the
one who notices that and pulls it back: *"this has got more complicated than it needs
to be,"* or *"we already settled that — let's not relitigate it."* Naming the drift
plainly and redirecting is part of the job, and it keeps a long build on course.

### 4. Know when to stop

Some work should not be done tired, and some should not be rushed because you're
close to the end. Cryptographic code especially: writing it at the tail of a long
session is exactly how subtle, serious bugs ship. In the Fieldnotes build, the
encryption work was deliberately *deferred* to a fresh session rather than finished
while flagging. Stopping was the disciplined call, not a failure of stamina.

You set the pace. "We'll do this properly next time, with a clear head" is a
legitimate and often correct decision.

### 5. Verify on the real artifact — and ask for tests you can't write yourself

"It compiles" is not "it works." "It works in the development version" is not "the
thing you actually ship works." The Fieldnotes encryption feature was proven not in
the dev build but on the *installed release* — download it, install it, run it, and
confirm the round-trip — before it was called done. The released alpha was launched
and signed into from the real installer before it was published.

The sharpest example was the administrator lock — the most important rule in the
whole app, that only an authorised admin can add scenarios or view screenshots. Built
as a variation of ProofPoll's, it *looked* right: the app complied, the buttons
behaved exactly as expected. It wasn't actually locking — it was letting things
through while silently failing. In an app about secure identity, that's serious.

What caught it was not insight; it was a *test*. Proving the lock meant checking two
people on one network — one authorised, one not — and one person with one computer
can't be two genuinely separate peers without a test harness. That's a thing you
can't write yourself as a non-coder — so you ask for it. The test failed (the
unauthorised peer got in), the fault was traced and fixed, and the test passed (the
intruder refused, and even faking someone else's permission was blocked). Now there's
*proof* the lock holds.

The lesson: insist on the last mile, and run tests as you build and before you share.
You don't have to write them by hand — but you do have to know to *ask* for them. The
gap between "it looks like it works" and "it works" is exactly where confident,
untested answers live.

And there's a class of problem that *no* automated test will catch for you:
**visual and layout errors.** Sweettests, type checks, and compiler checks prove the
logic and the wiring, but they're blind to whether a screen actually looks right — a
button rendered in the wrong place, a broken layout, a panel that's silently empty.
All of that passes every automated check and only *your eyes* catch it. So look at
the running app often as you build — not just at the end. It costs minutes, it
catches the visual class of bug early, and it's the one kind of checking the AI
simply cannot do for you, because it can't see the screen.

### 6. Use the standard path; don't hand-roll

This one was learned the hard way on release day, and it is the most broadly useful
lesson here. Fieldnotes inherited ProofPoll's release pipeline — which was built for
ProofPoll's situation, including code-signing certificates that ProofPoll's authors
have and you may not. Trying to coax that inherited, signing-heavy pipeline into
working for an *unsigned* alpha meant fighting it failure by failure for far too
long.

The fix was a single realisation: *you are not the upstream author, you do not have
their certificates, and you do not need them for an alpha.* The answer was to throw
out the bespoke, inherited complexity and use the bog-standard, off-the-shelf
unsigned build pipeline that every new developer uses — hundreds of lines of
inherited signing machinery replaced with a few dozen lines of the documented
standard. It worked first time.

The transferable lesson: **know what you actually need versus what you inherited.** A
fork carries the original author's assumptions baked in. When something fights you
relentlessly, the question to ask is not "how do I make this complicated thing work"
but "do I even need this, or is there a standard, simpler path for *my* situation?"

### 7. Watch for the pull toward the old, centralised shape

This is the deepest principle, and the one most specific to building on Holochain. An
AI works by probability: it has absorbed an enormous quantity of the world's existing
software, and when asked to build something it leans toward the most common patterns
it has seen. Almost all of that is the *old* way — central servers, central
databases, everyone's data pooled where one party controls it. Holochain is
deliberately none of that, and the further you lead the model into genuinely novel,
agent-centric territory, the harder it pulls back toward the familiar.

So at many steps, the AI will fluently propose the centralised shape, and it will
look perfectly reasonable. You have to catch it. In the Fieldnotes build it reached
for the old default three times, and each time the right answer was the opposite:

- Asked how to know who was using the app, it proposed **a central list of users** —
  but there is no central database to hold one. (The identity broker resolves this
  dynamically instead.)
- Asked to remove a draft, it proposed **deleting it** — but the network is immutable
  by design. (The right answer was an *archive* that hides without destroying.)
- Building the administrator lock, it would have let a privileged account simply
  **hold hidden power** — where the right path was to declare that authority openly,
  for the whole network to verify and choose to cooperate with.

Notice what those choices actually are. They look like technical details — storage,
deletion, permissions — and they are nothing of the kind. Each is a choice *about
people*: who controls identity, who may see your record, whether power is hidden or
accountable. The conventional answer, the one the AI reaches for every time, is
conventional precisely because most software was built without ever asking the human
question.

This is the real reason an app like this cannot be handed to an AI to produce on its
own. The important choices are not merely technical; they are ethical, made decision
by decision, and a model averaging all the software it has seen will reliably pick
the old default. **Your job is to reason through each of those choices rather than
let them pass unquestioned** — to ask, at every step that touches data, identity, or
power: *is this the centralised shape out of habit, and is that what I actually
want?* Humane, agent-centric software has to be chosen, not averaged into being. That
choosing is the part only you can do.

---

## Part 2 — The practical path

Here is the shape of the work, fork to alpha. None of it requires you to write code;
all of it requires the method above.

### The kit you need

- **ProofPoll** — the app you fork. It solved the genuinely hard parts of a desktop
  Holochain app (the conductor lifecycle, Flowsta identity, DNA migration, encrypted
  private data) so you don't have to. It was explicitly built to be forked.
- **Flowsta Vault** — the identity layer. Your app's users sign in with it, and your
  network's administrator authority is anchored to a Flowsta identity. Install the
  Vault desktop app; you'll need it to sign in and to test.
- **[Sacha's Holochain agent skill](https://github.com/Soushi888/holochain-agent-skill)** —
  a skill file that gives your AI the Holochain-specific technical grounding (zome
  structure, validation, testing, deployment — the full development spiral). It
  follows the Agent Skills open standard, so it works with Claude and other
  assistants. Add it to your AI's setup so it starts already fluent in the stack.
- **This method**, given to your AI as the payload in Part 3.
- **A terminal** — you'll run commands one at a time and paste the output back to
  your AI. You don't need to understand the commands; you need to run them and report
  honestly what came back.

### The steps, in order

1. **Fork ProofPoll** into your own repository. You inherit its whole structure and
   history.
2. **Get it running locally and set up Flowsta.** Install the Vault app, create an
   identity, and confirm ProofPoll itself launches and signs in before you change
   anything. (Establish the baseline works first — see principle 5.)
3. **Understand the data model before touching it.** ProofPoll's model is polls and
   votes. Have your AI explain how that model is shaped *from the real files* (see
   principle 1) before you decide how to change it.
4. **Swap the model for your domain.** This is the heart of the work: re-conceiving
   ProofPoll's poll/vote structure as whatever your app is. (Fieldnotes turned it
   into test scenarios, verdicts, and findings.) Done in guarded, reviewable edits.
5. **Add what's yours.** The features that make your app *your* app — built on the
   inherited foundation, in small verified steps.
6. **Rebrand.** The name, the icon, the identifiers, the on-screen text. A focused
   pass, with the AI verifying nothing was missed.
7. **Test it for real.** Get it running, exercise the features, and — where the stack
   supports it — have your AI write automated tests that *prove* the important
   guarantees hold (Fieldnotes proved its administrator enforcement with a test
   running two separate agents).
8. **Release an unsigned alpha.** Build installers for the platforms you want. Do not
   block your alpha on code-signing certificates you don't have (principle 6) — use
   the standard unsigned build pipeline. Verify the *installed* build launches before
   you publish (principle 5).

### The honest texture

It is not all smooth, and a guide that pretended otherwise would be lying. Two beats
from the Fieldnotes build worth knowing about in advance:

- **A wall you don't expect.** The encryption work hit a hard, undocumented size
  limit that broke the obvious approach. It was only found by reading the real error
  — twice — rather than trusting the assumption. Walls like this are normal; the
  method is how you get through them.
- **A fight that taught the real lesson.** The release pipeline (principle 6) fought
  hard before the simple, standard answer became obvious. Expect at least one of
  these. The lesson is usually "step back and use the standard path," not "try
  harder at the complicated one."

---

## Part 3 — Give this to your AI

Paste the block below into your AI assistant — as a project instruction, a custom
skill, or simply the first message of your build. It sets the AI up to work in the
way described above. Combine it with a Holochain skill (for the technical knowledge)
and you have an assistant that knows both the stack and how to work safely with you.

---

> **How to work with me on this build**
>
> I am a product owner, not a programmer. I understand what I want to build and why,
> and I can run terminal commands and report their output, but I cannot read or
> verify code myself. Your job is the reading, design, and code; my job is judgement,
> direction, and deciding when something is right. Work this way:
>
> 1. **Read the real source before reasoning.** Before you reason about any file or
>    behaviour, read the actual file or the actual error output — never reason from
>    memory about what the code probably says. If you haven't seen it, ask me to run
>    a command to show it to you. If you're uncertain where something lives, ask me
>    to find it (a directory listing, a search) rather than guessing. Silently
>    reasoning from memory is the failure I most need you to avoid. When you genuinely
>    can't see something — a file, the current state of the code, a page on the web —
>    say so plainly and ask me to fetch it; never reason about it from memory and
>    never assume it's simply unavailable. You don't touch my files directly; every
>    change passes through me running a command and seeing the result. Treat that
>    separation as a feature, not a limit — it's what keeps you from reaching past me
>    into the code, and keeps me in the loop on everything that changes.
>
> 2. **Make guarded, reversible edits.** Never rewrite a file blind. For each change,
>    use a script that anchors on an exact, unique piece of text, refuses to run if
>    that anchor isn't found exactly once, writes to a temporary copy first, and
>    shows me the difference to approve *before* the real file is touched.
>
> 3. **Push back on me.** If you disagree, or think I'm about to make a mistake, say
>    so plainly. Don't defer to me to be agreeable. I want honest correction in both
>    directions. If you find yourself agreeing with everything, something is wrong.
>
> 4. **Tell me when to stop.** If we're doing something risky — especially anything
>    cryptographic or security-related — late in a long session, say so and recommend
>    we do it fresh. Knowing when not to push is part of doing this safely.
>
> 5. **Verify on the real artifact, and write tests I can't.** "It compiles" is not
>    "it works," and "it works in development" is not "the shipped version works." For
>    anything important, confirm it on the actual built, installed result before
>    calling it done. And where a guarantee matters — especially security rules —
>    write automated tests that *prove* it holds (I can't write these myself, so
>    propose them), and run them as we build and before we share. Just as important:
>    have me *look at the running app often* as we go. Automated checks catch logic
>    and type errors, but they miss visual and layout problems entirely — a broken
>    screen can pass every test. My eyes on the real thing are the only check for that
>    whole class of bug, and looking early catches them before they pile up.
>
> 6. **Prefer the standard, off-the-shelf path.** This project is a fork, so it
>    carries the original author's assumptions and tooling. When something fights us
>    repeatedly, don't grind at the inherited complexity — stop and ask whether I
>    actually need it, or whether there's a simpler standard path for my situation. I
>    am not the upstream author and don't have their accounts, certificates, or
>    infrastructure, and usually don't need them for an alpha.
>
> 7. **Watch for the centralised default — flag it, don't pick it silently.** You'll
>    tend toward the common patterns you've seen most: central servers, central
>    databases, hidden authority. This is a Holochain app and usually needs the
>    opposite — no central store, immutable data, authority that's open and verified
>    rather than hidden. Whenever a step touches data, identity, or who-can-do-what,
>    name the choice explicitly and tell me the agent-centric alternative, so I can
>    decide rather than have the old default chosen for me.
>
> 8. **One step at a time.** Give me one command, let me run it and paste the output,
>    and read that output before the next step. Don't batch commands I'd run blind.
>    Explain the *why*, not just the mechanics. And before we start something big,
>    help me break it into a logical sequence of steps, and let's work through them in
>    order.

---

## Where this leads

If someone who can't write code can produce a working peer-to-peer application on
this foundation — and find, fix, and harden a real bug in it along the way — then the
barrier everyone assumed stood between an idea and a shipped Holochain app is lower
than it looks. ProofPoll lowered the floor. The method above is how you build from
it. What you make is up to you.

---

*Fieldnotes is licensed under CAL-1.0 and derives from ProofPoll (MIT). See the
[Fieldnotes repository](https://github.com/happenings-community/Fieldnotes) for the
app this guide came out of.*
