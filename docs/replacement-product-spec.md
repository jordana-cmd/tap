# From PlanSwift's Ashes: Designing the Definitive Takeoff & Estimating Platform

**Prepared as:** Product strategy synthesis of three independent research reports (Grok, Gemini, Claude analyses of PlanSwift) → full specification for the ideal replacement product.

**Adaptation note:** All three reports analyze the same product — PlanSwift by ConstructConnect — rather than three competitors. This is treated as a strength: three independent sources triangulating the same product yield unusually high-confidence findings. The reports also contain substantial competitive intelligence on STACK, Bluebeam Revu, On-Screen Takeoff, zzTakeoff, and Togal.ai, which is used to design a product that beats not just PlanSwift but the entire current field.

---

## PART 1 — THOROUGH ANALYSIS OF EACH REPORT

### Report A: Grok Review

| Dimension | Findings |
|---|---|
| **Core features/value** | Point-and-click takeoff (linear, area, count, volume); drag-and-drop assemblies with waste/labor formulas; Excel export; 2026 Takeoff Boost AI (Auto Takeoff "~80% head start," Auto Count, Auto Scale, Auto Bookmark) |
| **Strengths** | Takeoff speed ("jobs that took an hour now under 15 minutes"); assembly customization; smooth Excel/reporting; low learning curve for blueprint veterans; digitizer support |
| **Weaknesses** | 32-bit / ~2GB RAM ceiling → crashes, freezes, data loss on large plans; mixed-to-poor support ("will call back" no-shows); 2025 forced subscription shift ("bait & switch"); no cloud/collab/mobile; dated UI; add-on cost creep ($99–$349 per trade plugin) |
| **Target segments** | Solo/small-team (1–10) Windows estimators in trades: concrete, drywall, electrical, flooring; high-volume repeatable bidders |
| **Quantitative data** | Capterra ~4.3/5 (400+ reviews); G2 ~4.3/5; ~86–88% positive recommendation; support/reliability sub-scores ~4.1; price $1,749–$2,000/yr. Scores: Overall 7.2/10; Features 8.5; Reliability 6.0; Support 6.5; Value 7.0; Modern capabilities 5.5 |

### Report B: Gemini

| Dimension | Findings |
|---|---|
| **Core features/value** | Assembly engine that converts one traced wall into full material + labor bill (drywall sheets, studs, insulation, fasteners, hours); **Excel Live Link** (real-time data connection, not just export); area deductions, pitch corrections; QuickBooks link |
| **Strengths** | 35–40% faster than PDF markup tools for pure takeoff (Exayard); 93–96% of positive reviews cite time savings + assembly depth; visual precision for tenders |
| **Weaknesses** | **The definitive technical diagnosis:** hard 2.0 GB RAM cap (per ConstructConnect's own support docs) → out-of-memory crashes on large/multi-page/high-DPI sets; **Data.xml corruption disasters** (crash mid-write corrupts job files; vendor's own recovery docs warn uninstalling deletes unarchived jobs); 67% negative phone-support sentiment; local file locking blocks any simultaneous work; steep assembly setup; static desktop pricing can't track 2025–26 material cost volatility |
| **Target segments** | Small-to-midmarket specialty subs (roofing, drywall, MEP, concrete, pavement); independent estimating consultants with formula-heavy assemblies |
| **Quantitative data** | Sentiment: 68% positive / 12% neutral / 20% negative, declining over 24 months. Scores: Assembly depth 9.5/10; Reliability 4.5/10; Support 5.5/10; Value 6.0/10; Cloud 3.0/10 |

### Report C: Claude

| Dimension | Findings |
|---|---|
| **Core features/value** | Same core + broad file support (.TIF/.PDF/.DXF/.DWF/.DWG/.PLN/.JPG, plan-room import); Takeoff Boost detail (tested 3 yrs in OST first; **gated to Premium/Core subscriptions** — Standard gets no AI, i.e., AI used as subscription bait) |
| **Strengths** | "If it's colored, it's counted" intuitiveness; "cut our work time in half"; sticky assembly libraries; flooring named as a genuine sweet spot; G2 rates it easier to set up than STACK |
| **Weaknesses** | Crash-plus-unreachable-support failure mode at bid deadlines; fragile activation (must deactivate licenses before OS upgrades); pricing fog (conflicting published prices, opaque bundling); incentivized-review inflation on Capterra |
| **Unique red flags** | (1) **License trust rupture with receipts:** 2017 "lifetime" perpetual buyers deactivated without notice in 2025; corroborated by ConstructConnect's own Oct 2022 sunsetting of PlanSwift ≤10.1 activation — "your lifetime license dies when the activation servers stop honoring it." (2) **EULA data grab:** perpetual, worldwide, royalty-free license to use customer plan content to train vendor ML models, with vendor exclusively owning aggregate content — material for firms whose vendor also runs a bid-data business. (3) Fake AI-generated hype content circulating about the product |
| **Quantitative data** | Capterra 4.3 (396 reviews), ease 4.4, support 4.1, value 4.1; G2 4.3; 60.6% small biz, 27.3% mid-market; 83% construction pros, estimating 58% of use. Distribution est. ~70–75% pos / 15% neutral / 10–15% neg, negativity clustering 2024–26. Scores: Takeoff 8; Reliability 5.5; Support 5.5; Value 6; Modern 5 → Overall 6.3/10 |

---

## PART 2 — CROSS-REPORT SYNTHESIS

### 2.1 Consensus findings (all three reports agree — treat as ground truth)

1. **The assembly engine is the crown jewel.** All three independently identify formula-driven assemblies (trace once → full material/labor/waste bill) as the reason users stay despite everything. Depth scored 8.5–9.5/10 across reports. **Any replacement must match or exceed this on day one.**
2. **Excel is not a feature; it's the workflow.** Estimators run decades of proprietary pricing logic in spreadsheets. Gemini's "Live Link" framing is key: the winning integration is bidirectional and real-time, not export-only.
3. **The 32-bit architecture is the root cause of the #1 complaint.** Crashes/freezes/data loss on large plan sets, confirmed by the vendor's own documentation (2GB cap). This is the single largest churn driver among power users.
4. **Trust is broken.** The 2025 perpetual-license revocation converted the most loyal cohort into the loudest detractors. This is a *market-wide opportunity*: thousands of trained, assembly-fluent estimators are actively shopping with a grudge.
5. **No cloud, no collab, no mobile, no Mac** — the entire modern-workflow surface is conceded to STACK/Bluebeam.
6. **Support fails exactly when it matters most** (bid deadlines), and pricing is foggy with plugin cost creep.

### 2.2 Complementary/unique intelligence per report

| Insight | Source | Strategic use |
|---|---|---|
| 2GB RAM cap + Data.xml corruption mechanics, vendor-documented | Gemini | Defines the reliability bar and the marketing attack ("never lose a takeoff again") |
| EULA trains vendor AI on customer bid data | Claude | Defines the **data-privacy differentiator**: "your plans never train our models without opt-in" |
| Sunsetting policy proves activation-server dependency kills "perpetual" | Claude | Defines the **trust guarantee** feature set (escrowed offline mode, data portability) |
| AI gated by tier as subscription bait; 80% head-start claim unverified in field | Grok + Claude | AI must be included in all paid tiers and honest about accuracy |
| Static desktop pricing can't track material cost volatility | Gemini | Opportunity: **live commodity pricing feeds** — no competitor in the reports does this |
| zzTakeoff (ex-PlanSwift talent, ~$550–600/user/yr) winning switchers | Grok | Price ceiling signal + proof that migration-friendly positioning works |
| Flooring/coatings named sweet spot; sticky users = switching-cost moat | Claude | Migration tooling (import PlanSwift assemblies/jobs) neutralizes the incumbent's only moat |

### 2.3 Strength-for-weakness mapping across the competitive field

| Weakness in market | Who partially solves it today | How the new product wins outright |
|---|---|---|
| PlanSwift crashes on large sets | STACK (cloud rendering) | Local-first 64-bit engine + GPU tiled rendering: desktop speed *and* no memory ceiling |
| STACK's shallower formula customization | PlanSwift assemblies | Full assembly engine, cloud-synced, versioned, shareable |
| Bluebeam's weak native estimating | PlanSwift/STACK | Estimating-first design with Bluebeam-grade PDF handling |
| No one has trustworthy AI | Togal (AI-first, pricey), Takeoff Boost (gated, unverified) | Verifiable AI: confidence scoring, click-to-audit provenance per measurement, included in every paid tier |
| No one has live cost data | — (gap across all reports) | Integrated regional material price feeds + one-click re-price of entire bid |
| Everyone locks data in | zzTakeoff (partial, cheaper) | Contractual data portability + open export + offline escrow mode |

---

## PART 3 — THE REPLACEMENT PRODUCT SPECIFICATION

### 3.1 Product name options

1. **TrueBid** — leans into the trust rupture; "the estimating platform that keeps its promises."
2. **Assembly** — names the crown-jewel feature; clean, ownable, developer-brandable (Assembly Cloud, Assembly Field).
3. **QuantaTake** — quantity + takeoff; technical, precise, trade-credible.

**Recommended: Assembly.** It signals to PlanSwift refugees, in one word, that the thing they can't leave behind lives here.

### 3.2 Core value proposition / tagline

> **"Trace once. Bid everything. Lose nothing."**

Sub-positioning: *The assembly-driven takeoff platform with desktop speed, cloud teamwork, verifiable AI — and a license that can never be revoked out from under you.*

### 3.3 Feature set

#### Must-have (launch parity + fixes — Year 1)

| Feature | Spec | Weakness it kills |
|---|---|---|
| **64-bit local-first engine** | Native Windows + macOS app; GPU-accelerated tiled/streamed PDF rendering; tested against 5GB+ multi-hundred-page civil sets; memory-mapped paging, no hard RAM ceiling | 2GB cap, crashes, freezes |
| **Crash-proof persistence** | Append-only journaled project store (SQLite WAL, not monolithic XML); autosave every action; point-in-time recovery; continuous encrypted cloud snapshot | Data.xml corruption, "lost days before a bid" |
| **Full assembly engine** | Drag-and-drop assemblies; nested formulas (waste %, labor rates, pitch/thickness corrections, area deductions); condition-based logic; assembly versioning and diff | Must match PlanSwift's 9.5/10 depth |
| **Excel Live Link 2.0** | Bidirectional real-time sync to Excel *and* Google Sheets; named-range mapping; works with legacy hard-coded pricing sheets unchanged | Preserves the workflow estimators refuse to abandon |
| **PlanSwift Migration Kit** | One-click importer for PlanSwift jobs, assemblies, parts, templates, and plugin data; free white-glove migration for annual customers | Neutralizes incumbent's switching-cost moat; harvests the angry cohort |
| **All core takeoff modes** | Linear, area, count, volume, cutout/deduction, auto-scale detection, digitizer support; file support: PDF, TIF, DWG, DXF, DWF, JPG, PNG, plan-room import | Table stakes |
| **Real-time collaboration** | Multiple estimators in one plan set simultaneously (CRDT-based sync); presence, per-page locking optional, comment threads, @mentions | "Pass files back and forth" era ends |
| **The License Charter** | Contractual guarantees: (1) offline escrow mode — if activation servers ever go dark or company folds, software converts to perpetual offline license automatically; (2) full data export in open formats at any time; (3) 3-year price-increase cap in contract; (4) customer data never trains vendor AI without explicit opt-in | The entire trust rupture, weaponized as product |

#### High-value differentiators (the reasons to switch — Year 1–2)

| Feature | Spec |
|---|---|
| **Verifiable AI Takeoff** | Auto-detect walls/doors/fixtures/areas across a full sheet; every AI measurement carries a confidence score and a one-click audit view (zoom to source geometry); accept/reject queue; accuracy telemetry published quarterly — an honest counter to unverified "80% head start" claims. Included in **every paid tier.** |
| **Live Cost Intelligence** | Regional material price feeds (lumber, steel, concrete, drywall, copper, coatings) piped into assemblies; one-click re-price of an entire bid when markets move; price-lock date stamping on proposals; volatility alerts on open bids |
| **Field Companion (iPad/Android/web)** | Read-write mobile viewer: field crews see live takeoffs on-site, verify dimensions, drop photo-pinned annotations that sync back to the estimator |
| **Bid-Deadline SLA** | Support tier with guaranteed 15-minute response during customer-declared bid windows; 24/7 chat with real technicians; screen-share by default |
| **Assembly Marketplace** | Community + vendor-published trade assembly packs (concrete, roofing, drywall, MEP, flooring/coatings) — free with subscription, revenue-share for creators; replaces PlanSwift's $99–$349 plugin nickel-and-diming |
| **Revision Radar** | Automatic overlay/diff of plan revisions; changed regions highlighted; affected takeoff items flagged for re-measure — addresses revision-tracking gap all three reports note |

#### Nice-to-have (Year 2–3)

- BIM/IFC quantity extraction (Revit model → assembly quantities)
- Procore, Buildertrend, QuickBooks Online, Sage 300 CRE native integrations + public REST/GraphQL API with webhooks
- Proposal generator (branded PDFs from takeoff data)
- Subcontractor bid-leveling module
- Multi-language / metric-imperial dual display
- Voice takeoff annotation in field app

### 3.4 Technical architecture

**Philosophy: local-first, cloud-synced.** Desktop-class rendering performance (what PlanSwift users demand) with cloud durability and collaboration (what they defect for). Never make the user choose.

| Layer | Recommendation | Rationale |
|---|---|---|
| **Client core** | Rust engine compiled native (Win/macOS via Tauri) + WebAssembly build for browser client; shared geometry/measurement kernel across all platforms | One kernel, three surfaces; 64-bit, memory-safe, no crash class |
| **Rendering** | GPU-accelerated (wgpu) tiled vector rendering; progressive streaming of large sheets; background rasterization workers | 4K multi-page civil sets scroll at 60fps |
| **Local store** | SQLite with WAL journaling per project; content-addressed page cache | Crash-proof, corruption-resistant, offline-complete |
| **Sync/collab** | CRDT document model (Automerge/Yjs-class) over WebSocket; end-to-end conflict-free merge; works offline, syncs on reconnect | Real-time multi-estimator without file locking |
| **Cloud backend** | Kubernetes on major cloud; PostgreSQL (tenant data), S3-class object store (plans, snapshots), Redis (presence) | Boring, scalable, provably reliable |
| **AI pipeline** | Vision transformer models for symbol/region detection, fine-tuned per trade; inference server-side with on-device fallback for privacy tier; human-in-the-loop labels only from opted-in customers | Verifiable AI + honors the data promise |
| **Pricing feeds** | Ingest layer normalizing commodity indices + regional supplier APIs; per-region price tables versioned daily | Live Cost Intelligence |
| **Security** | SOC 2 Type II from Year 1; AES-256 at rest, TLS 1.3 in transit; SSO/SAML for teams; per-project encryption keys; optional customer-managed keys for enterprise | Bid data is competitively sensitive — the reports show users already worry about vendor data use |
| **Licensing infra** | Signed offline license tokens with escrow trigger (dead-man's-switch smart contract or third-party escrow agent honoring the License Charter) | Makes "we can't pull a PlanSwift on you" technically enforceable, not just promised |

### 3.5 Pricing model

Principles from the synthesis: transparent (kill the pricing fog), no plugin nickel-and-diming, undercut PlanSwift's $1,749–2,000 while overdelivering, land above zzTakeoff's ~$550–600 floor on value not price.

| Tier | Price | Includes |
|---|---|---|
| **Solo** | **$89/user/mo ($948/yr annual)** | Full takeoff + assemblies + Excel Live Link + AI takeoff + all trade packs + mobile viewer (2 free field seats) |
| **Team** | **$129/user/mo ($1,308/yr)** | Everything in Solo + real-time collaboration + Revision Radar + Live Cost Intelligence + admin controls |
| **Business** | **$179/user/mo ($1,788/yr)** | Everything + Bid-Deadline SLA + SSO + API access + customer-managed keys + white-glove PlanSwift migration |
| **Field seats** | Free (view/annotate) | Drives adoption beyond the estimating desk |
| **Trial** | 30 days, full-featured, load-your-largest-plan-set challenge | Directly answers all three reports' "trial with your real plans" advice |

Every tier gets AI. Every tier gets every trade pack. Every tier gets the License Charter. Switcher offer: 12 months at 40% off with proof of an active PlanSwift subscription + free migration.

### 3.6 Target personas & primary use cases

1. **"The PlanSwift Refugee" — Miguel, owner-estimator, commercial flooring/coatings sub (3 estimators).** Deep assembly library built over 8 years; burned by the 2025 license revocation; loses ~2 hrs/week to crashes and constant saving. *Use case:* migrate library intact, bid 30% more jobs via AI-assisted first pass, re-price bids when epoxy resin costs spike.
2. **"The Solo Consultant" — Dana, independent estimating consultant, multi-trade.** Lives in her hard-coded Excel pricing workbook; bills by the takeoff. *Use case:* Excel Live Link 2.0 with zero workbook changes; Mac support ends her Parallels misery.
3. **"The Growing GC Team" — Alvarez Construction, 8 estimators, mid-market commercial.** Currently split between Bluebeam (markup) and spreadsheets (estimating); rejected PlanSwift over collaboration. *Use case:* simultaneous multi-estimator takeoff on a hospital set; Revision Radar on addenda; field superintendents verify dimensions on tablets.
4. **"The Deadline Warrior" — any estimator, 9 p.m. the night before bid day.** *Use case:* the product's implicit hero scenario — nothing crashes, everything autosaved, support answers in 15 minutes, and the bid goes out.

### 3.7 UI/UX philosophy

- **"If it's colored, it's counted" is sacred.** The visual-immediacy metaphor users praise stays central: every measurement is visibly painted, every painted region is quantified.
- **Progressive depth.** Point-and-click productive in the first hour (matching PlanSwift's praised low entry curve); assembly authoring revealed progressively with templates, a formula assistant, and an interactive setup wizard — attacking the "not plug-and-play" setup complaint.
- **Never block the estimator.** All sync, AI inference, and rendering is background/async; the trace cursor never stutters. Perceived performance is the brand.
- **Trust surfaces everywhere.** Autosave indicator, last-snapshot timestamp, AI confidence badges, price-feed freshness stamps — the UI constantly proves the product's reliability claims.
- **Keyboard-first for power users; touch-first in field app.** Two honest modalities rather than one compromised hybrid.
- **Dated-UI antidote without novelty for its own sake:** high-contrast plan canvas, dockable panels, dark mode, but zero disruption to the takeoff muscle memory PlanSwift veterans carry.

---

## PART 4 — FUTURE-PROOFING PLAN (3–5 YEAR ROADMAP)

### Phase 1 — "Parity + Trust" (Months 0–12)

- Ship: 64-bit engine, full assembly system, Excel Live Link 2.0, crash-proof persistence, PlanSwift Migration Kit, License Charter, Solo/Team tiers, Windows + macOS.
- GTM: target the refugee cohort — trade forums, r/estimating, flooring/concrete/drywall associations; "largest plan set challenge" demo campaign.
- Success metrics: 5,000 paid seats; <0.1% crash rate per session; NPS > 50; 500 migrated PlanSwift libraries.

### Phase 2 — "Collaboration + Intelligence" (Months 12–24)

- Ship: real-time collab GA, Verifiable AI Takeoff GA with published accuracy reports, Live Cost Intelligence (top 10 commodity categories, US regions), Field Companion apps, Revision Radar, Assembly Marketplace beta, browser client.
- Success metrics: 40% of accounts multi-seat; AI adoption in 60% of takeoffs; marketplace with 200+ assembly packs.

### Phase 3 — "Platform" (Months 24–36)

- Ship: public API + webhooks, Procore/QuickBooks/Sage integrations, BIM/IFC extraction beta, proposal generation, enterprise tier (SSO, CMK, audit logs), Canada/UK/AU regional pricing feeds + metric support.
- Success metrics: 20% of new revenue via integrations/marketplace; 3 enterprise logos > 50 seats.

### Phase 4–5 — "Agentic Estimating" (Years 3–5)

- **Agentic bid assistant:** given a plan set + scope letter, agent proposes a complete draft takeoff and bill of materials overnight; estimator reviews via the audit queue. (The trajectory Togal points at, executed with the verification UX and trust posture competitors lack.)
- **Predictive estimating:** win-rate and margin analytics from anonymized *opted-in* data; "bids like this one historically win at X% with Y margin."
- **Edge/on-device AI tier:** full inference locally for security-sensitive government/industrial estimators — a segment no cloud-AI competitor can serve.
- **Ecosystem moat:** marketplace revenue share turns the trade community into the product's R&D arm.

### Emerging-tech integration watchlist

| Trend | Integration path |
|---|---|
| Multimodal foundation models | Scope-letter + spec-book comprehension feeding assembly selection |
| Agentic systems | Overnight draft takeoffs; auto-RFI drafting when plans conflict |
| No-code | Visual assembly/formula builder; Zapier-class automation recipes |
| Privacy tech | On-device inference tier; customer-managed keys; opt-in-only training data (already core) |
| Edge computing | Field app offline AI measurement verification via phone camera + LiDAR |
| AR | Overlay takeoff geometry on physical site through tablet camera (Year 4 experiment) |

### Data & integration strategy

- **Open by default:** documented export schemas (CSV, JSON, IFC-adjacent) for every object; the anti-lock-in stance *is* the moat because it's the one thing an incumbent with a data-monetization business model cannot copy.
- **Ingest everything:** plan rooms, cloud drives, email-to-project intake, Bluebeam Studio session import.
- **Training data only from explicit opt-in**, compensated with subscription credits — converts the EULA red flag from Report C into a recruiting advantage.

### Continuous improvement loops

- In-app telemetry on crash/latency budgets with public quarterly reliability reports (radical transparency vs. the incumbent's opacity).
- AI accuracy scored against estimator corrections; per-trade model retraining quarterly.
- Customer council of 25 estimators across trades with quarterly roadmap votes.
- Support ticket taxonomy reviewed monthly; any category >5% of volume triggers a product fix, not a knowledge-base workaround (the exact anti-pattern Gemini's report flagged).

### Risk register & mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| ConstructConnect ships 64-bit rewrite + bundles OST AI | Medium | Speed: their 3-year OST-to-PlanSwift AI port shows slow cross-porting; trust damage is unfixable by features — keep the License Charter front and center |
| STACK moves down-market on assemblies | Medium | Assembly depth + marketplace network effects; migration kit for STACK too by Year 2 |
| AI accuracy disappoints → credibility hit | Medium | Never market unverified percentages; confidence-scored, audit-first UX; publish real accuracy data |
| Price war from zzTakeoff (~$550/yr floor) | High | Don't fight on price; fight on reliability SLA, cost intelligence, and collaboration — capabilities a low-price point can't fund |
| Material price-feed licensing costs/accuracy | Medium | Start with public indices + 2 supplier partnerships; expand regionally with revenue |
| Construction downturn shrinks seat counts | Medium | Solo tier + consultant persona keeps a counter-cyclical base (downturns *increase* competitive bidding volume) |
| Regulatory: data-residency (gov/infrastructure bids) | Low-Med | Region-pinned storage + on-prem/edge tier in Phase 4 |

---

## VERDICT

The three reports converge on a rare market setup: a category leader with a **beloved core (assemblies + Excel), a vendor-documented fatal flaw (32-bit crashes + data corruption), and a self-inflicted trust catastrophe (license revocation + AI data grab)** — surrounded by competitors that each solve only one piece. The winning product is not an incremental cloud clone; it is **PlanSwift's soul (assembly depth, Excel fidelity, visual immediacy) in a body it never had (64-bit local-first engine, real-time collab, verifiable AI, mobile field access) under a covenant it broke (the License Charter).** Execute Phase 1 with migration tooling aimed squarely at the refugee cohort, and the incumbent's only moat — switching cost — becomes the new product's primary customer-acquisition channel.
