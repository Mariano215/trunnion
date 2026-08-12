# Console visual direction

Three directions for the trunnion console. Pick one; every token in the build derives from it.

Constraint carried in from the plan: the console stays framework-free. No Vite, no React, no build step. These directions are CSS and markup against `assets/`.

## What the data forced

Live scans of five real repositories (`trunnion`, `claude-harness-core`, `ClaudePaw`, `claude-dashboard`, `harness-kit`) returned **overall 0 for all five**. That is correct — the minimum rule is the thesis, and every real repository has at least one primitive at zero — but it means:

1. **The composite cannot be the headline.** A number that reads 0 for everyone carries no information. The hero has to be the *profile* across twelve and the identity of the floor primitives.
2. **The actionable number is distance, not level.** "Four primitives hold the rail on the floor" and "lift these three to reach level 2" are what a person can act on.
3. **The static ceiling of 3 should be visible, not stated.** `src/scan.rs:710` explains in prose that levels 4 and 5 are unreachable without telemetry. Draw it: the region above 3 is a zone the static scan cannot enter.
4. **Non-agentic repositories need their own signal.** `harness-kit` scores eleven zeros and is *correctly* non-agentic. Reading identically to a badly-built agent platform is a defect. Surface "no agentic surface detected" rather than implying failure.

---

## Direction A — Load-bearing  *(recommended)*

The subject is a trunnion: a rigid overhead frame that carries a moving load. It does not do the work. It holds the thing that does, and defines where it can go. The vernacular is structural engineering — plate lettering, hazard marking, tolerance bands, load paths.

**The minimum rule becomes gravity.** Twelve bars stand at their level. A horizontal rail rests across the top of the *shortest* one. That is the overall level, and it is not a printed number, it is where the rail physically sits. The bars touching the rail are hazard-marked: they are load-bearing, and lifting any one of them does nothing until all of them rise together.

### Tokens

```
--ground   #E7E5E0   warm concrete, the page
--plate    #F6F5F2   raised panel
--plate2   #EFEDE8   recessed well
--ink      #15171A   graphite, primary text
--steel    #565E66   secondary text, structural members
--dim      #8A9099   captions, units
--rule     #CBC7BE   hairline
--rule2    #DDD9D1   faint hairline
--hazard   #F0B400   the floor. Used for nothing else.
--deny     #A93226   policy denial, in the trace view only
--ok       #2E6B4F   verified head, muted
```

Light industrial rather than dark. Deliberate: the three looks generated design currently defaults to are dark-with-one-accent, cream-with-serif, and broadsheet-with-hairlines. A light structural ground is the axis nobody spends. It also prints, emails, and drops into a deck without inversion, which a security console genuinely needs to do.

Hazard yellow does exactly one job. It marks the load-bearing primitives and appears nowhere else on the page. Spend the boldness in one place.

### Type

```
--display  "DIN Condensed", "Saira Condensed", "Archivo Narrow", "Avenir Next Condensed", Impact, sans-serif
--sans     system-ui, -apple-system, "Segoe UI", Roboto, sans-serif
--mono     ui-monospace, "SF Mono", Menlo, "IBM Plex Mono", Consolas, monospace
```

DIN is the German industrial standard face — technical drawings, road signs, machine plates. It is the native lettering of the object the product is named after. Set large, uppercase, tight, and used only for the wordmark and the primitive names on the chart.

Mono carries every number, hash, and path, because those are the things that must be copyable and comparable character by character.

**Ship note:** the offline guarantee forbids fetching fonts. DIN Condensed is macOS-only, so the stack degrades on Linux. Before release, embed one OFL condensed face (Saira Condensed or Archivo Narrow) as a woff2 via `include_bytes!`. ~40KB, keeps the binary self-contained.

### Layout

```
┌─ STATIC SCAN · CEILING 3 · NO TELEMETRY · 5 PROJECTS · READ-ONLY ────────┐  provenance bar
├──────────────────────────────────────────────────────────────────────────┤
│ TRUNNION                                      workspace · 5 projects     │  masthead
│ twelve primitives, a path behind every number   last scan 14:22Z         │
├──────────────────────────────────────────────────────────────────────────┤
│ [trunnion ▪▪▫▫▪▪▪▫▪▪▫▪ 0] [ClaudePaw ...] [harness-kit ...] [...]          │  project index
├──────────────────────────────────────────────────────────────────────────┤
│  5 ┌────────────────────────────────────────────────┐                    │
│    │////////  TELEMETRY REQUIRED  ///////////////// │  hatched, unreachable
│  3 ├──█──█──────────█──█──█─────────█──────────█────┤                    │
│  2 │  █  █  ·  ·    █  █  █  ·  █   █     ·    █    │                    │
│  1 │  █  █  ·  ·    █  █  █  ·  █   █     ·    █    │                    │
│  0 ╞══▀══▀══▓══▓════▀══▀══▀══▓══▀═══▀═════▓════▀════╡  ← RAIL rests here  │
│      01 02 03 04 05 06 07 08 09 10 11 12                                 │
│      ▓ = load-bearing, hazard marked                                      │
├──────────────────────────────────────────────────────────────────────────┤
│ 03 CONTEXT MANAGEMENT   0   looked in graphify-out/, retrieval.json, …    │  twelve rows
│                             gap: nothing on disk. …                       │
├──────────────────────────────────────────────────────────────────────────┤
│ LIFT ORDER   1. execution environment  2. tool interface  3. …            │  remediation
├──────────────────────────────────────────────────────────────────────────┤
│ CHAIN   no ledger. static evidence only. → trunnion instrument trunnion       │  chain panel
└──────────────────────────────────────────────────────────────────────────┘
```

### Signature

**The rail settling.** On load the bars grow from the floor, then the rail drops and comes to rest on the shortest one. Gravity, once, at the top of the page. Everything else on the page is still. `prefers-reduced-motion` renders the settled state with no animation.

Second signature, once a project is instrumented: **the real chain.** Dashboards in this space routinely draw a hash chain as ornament, filled with numbers computed for the look of it. Trunnion has an append-only log, a signed tree head, and offline verification. The chain panel shows the live head, links resolving as events append, and a verify action that genuinely recomputes. Until a project is instrumented, that panel states its own emptiness and names the command that fills it.

### The risk being taken

Light ground for a monitoring console. Consoles are dark by convention and the convention exists for a reason. This bets that trunnion is read in daylight by someone deciding whether to accredit a system, not stared at for eight hours, and that an assessment which survives being printed is worth more than one that glows.

---

## Direction B — The Chain

Dark. The hash chain is a literal vertical spine running the full height of the page; every panel attaches to it at its event, and scrolling walks the log.

```
--void     #12100E   warm near-black, not navy
--plate    #1B1815
--ink      #EDE8DF   bone
--steel    #8C857A
--rule     #2E2A25
--verified #7FB069   cold-free green, only on a verified head
--held     #E0A458   an approval waiting
--deny     #C4553F
```

Type mono-dominant: the whole page in monospace with one display face, because the product's truth-carrier is a hash and a hash is monospace.

The safest choice and the least distinctive one. Warm near-black instead of navy is the one move separating it from the dark console look generated design defaults to. Pick this when the console has to sit beside existing dark collateral without looking like a different company built it, and accept that it will read as the same page twice.

---

## Direction C — Refusal

Every dashboard makes green the hero. Trunnion's most distinctive act is refusing a tool call and **naming the rule that refused it**. This direction makes the deny state the most beautiful, most typographically considered moment on the page; everything else is deliberately quiet monochrome so the refusal is the only saturated thing a viewer ever sees.

Strong point of view, genuinely memorable, and a poor fit for a workspace overview where most of the page is scan results and no refusal has happened yet. Hold it for the trace view rather than the front page.

---

## Recommendation

**Direction A.** It is the only one whose central visual is derived from the product's actual thesis rather than applied to it — the rail *is* the minimum rule, not an illustration of it — and it solves the composite-is-always-zero problem that the live scan data exposed, instead of styling around it.
