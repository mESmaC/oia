# OIA implementation backlog

The v0.1 specification is broken into GitHub work packages. GitHub is the source of truth for issue state, assignments, and progress; these links index the backlog rather than duplicate its live status.

- [Implementation Project](https://github.com/users/mESmaC/projects/4): status and phase tracking.
- [Roadmap issue](https://github.com/mESmaC/oia/issues/1): phase hierarchy, requirement coverage, and decision coverage.
- [Repository issues](https://github.com/mESmaC/oia/issues): all work records.

The initial backlog contains 41 work packages, seven phase epics, and one roadmap issue. Together the work packages cover all 76 requirements and all 14 open decisions from the specification. Milestones express proposed delivery scope, not promised dates.

Start with [platform decisions and the Rust workspace](https://github.com/mESmaC/oia/issues/9) and the P0 foundation. Individual issues include prerequisites, scoped completion criteria, requirement snapshots, and acceptance scenarios. Some requirements span several work packages; completing one issue does not automatically complete its whole requirement.

The repository was empty on GitHub when this backlog was created, so issue bodies include the relevant specification text instead of broken links to unpushed files. Once the specification is published, add links to its immutable revision.

## Phase index

| Phase | Epic | Milestone |
| --- | --- | --- |
| P0: Evidence foundation | [#2](https://github.com/mESmaC/oia/issues/2) | [Delivery scope](https://github.com/mESmaC/oia/milestone/1) |
| P1: Document and news research | [#3](https://github.com/mESmaC/oia/issues/3) | [Delivery scope](https://github.com/mESmaC/oia/milestone/2) |
| P2: UAP, geographic, and space reconstruction | [#4](https://github.com/mESmaC/oia/issues/4) | [Delivery scope](https://github.com/mESmaC/oia/milestone/3) |
| P3: Media and connected observation | [#5](https://github.com/mESmaC/oia/issues/5) | [Delivery scope](https://github.com/mESmaC/oia/milestone/4) |
| P4: Analysis and collaborative fieldwork | [#6](https://github.com/mESmaC/oia/issues/6) | [Delivery scope](https://github.com/mESmaC/oia/milestone/5) |
| P5: Markets and general forecasts | [#7](https://github.com/mESmaC/oia/issues/7) | [Delivery scope](https://github.com/mESmaC/oia/milestone/6) |
| P6: Privacy tools and release expansion | [#8](https://github.com/mESmaC/oia/issues/8) | [Delivery scope](https://github.com/mESmaC/oia/milestone/7) |

## P0: Evidence foundation

- [#9: Resolve platform decisions and bootstrap the Rust workspace](https://github.com/mESmaC/oia/issues/9)
- [#10: Implement versioned evidence, claims, cases, and provenance records](https://github.com/mESmaC/oia/issues/10)
- [#11: Build transactional evidence storage, retention, and recovery](https://github.com/mESmaC/oia/issues/11)
- [#12: Implement shared time, coordinate, unit, and uncertainty primitives](https://github.com/mESmaC/oia/issues/12)
- [#13: Build source registry and bounded ingestion contracts](https://github.com/mESmaC/oia/issues/13)
- [#14: Implement durable jobs and shared application services](https://github.com/mESmaC/oia/issues/14)
- [#15: Establish Lean verification and the first Rust correspondence fixture](https://github.com/mESmaC/oia/issues/15)

## P1: Document and news research

- [#16: Build the investigation workspace and military-glass visual system](https://github.com/mESmaC/oia/issues/16)
- [#17: Implement document acquisition, OCR, and versioned extraction](https://github.com/mESmaC/oia/issues/17)
- [#18: Add document search, cited summaries, and grounded question answering](https://github.com/mESmaC/oia/issues/18)
- [#19: Track document releases, source lineage, and correction propagation](https://github.com/mESmaC/oia/issues/19)
- [#20: Construct regional news events from multiple sources](https://github.com/mESmaC/oia/issues/20)
- [#21: Build claim-checking and misleading-coverage review](https://github.com/mESmaC/oia/issues/21)
- [#22: Ship cited report drafting and portable case import/export](https://github.com/mESmaC/oia/issues/22)

  The dependency-independent [D05 decision](adr/0006-d05-report-and-bundle-formats.md)
  and [export contract](contracts/report-export.md) define the initial design;
  runtime implementation and A16/A17 acceptance remain open on #22.

## P2: UAP, geographic, and space reconstruction

- [#23: Build linked globe, history, regional clusters, and evidence views](https://github.com/mESmaC/oia/issues/23)
- [#24: Integrate weather, aircraft, maritime, and event context](https://github.com/mESmaC/oia/issues/24)
- [#25: Implement sky reconstruction and solar-system motion](https://github.com/mESmaC/oia/issues/25)
- [#26: Integrate satellite imagery, space observations, and mission footage](https://github.com/mESmaC/oia/issues/26)
- [#27: Build witness capture and historical report reconstruction](https://github.com/mESmaC/oia/issues/27)

## P3: Media and connected observation

- [#28: Integrate eligible camera directories, feeds, and retained clips](https://github.com/mESmaC/oia/issues/28)
- [#29: Detect and review camera anomalies with source-aware alerts](https://github.com/mESmaC/oia/issues/29)
- [#30: Build the image and video forensic workbench](https://github.com/mESmaC/oia/issues/30)
  - [Still-image diagnostic slice and remaining integration](implementation/image-forensics.md)
- [#31: Add predictive restoration and recreation-prompt export](https://github.com/mESmaC/oia/issues/31)
- [#32: Build radio listening with shared and local receivers](https://github.com/mESmaC/oia/issues/32)
- [#33: Connect personal drones with live telemetry and a UAV HUD](https://github.com/mESmaC/oia/issues/33)
- [#34: Create a personal observatory with synchronized sensor capture](https://github.com/mESmaC/oia/issues/34)

## P4: Analysis and collaborative fieldwork

- [#35: Implement deduction, extrapolation, correlation, and abstraction workflows](https://github.com/mESmaC/oia/issues/35)
- [#36: Build link analysis, collection planning, and research utilities](https://github.com/mESmaC/oia/issues/36)
- [#37: Build calibration, observation planning, and multi-view reconstruction](https://github.com/mESmaC/oia/issues/37)
- [#38: Add shared-case roles and offline field synchronization](https://github.com/mESmaC/oia/issues/38)
- [#39: Complete reproducible investigation bundles and analysis replay](https://github.com/mESmaC/oia/issues/39)
- [#40: Build the validation and simulation lab with operational benchmarks](https://github.com/mESmaC/oia/issues/40)

## P5: Markets and general forecasts

- [#41: Integrate point-in-time market data and public disclosures](https://github.com/mESmaC/oia/issues/41)
  - [Point-in-time normalization slice and remaining integration](implementation/market-point-in-time.md)
- [#42: Build market charts, exposure graphs, and event studies](https://github.com/mESmaC/oia/issues/42)
- [#43: Implement immutable forecasts, scenarios, outcomes, and evaluation](https://github.com/mESmaC/oia/issues/43)
- [#44: Forecast market effects from news, weather, releases, and scenarios](https://github.com/mESmaC/oia/issues/44)
- [#45: Forecast general event responses using historical precedents](https://github.com/mESmaC/oia/issues/45)

## P6: Privacy tools and release expansion

- [#46: Inspect and remove supported identifying file metadata locally](https://github.com/mESmaC/oia/issues/46)
- [#47: Audit personal public exposure and owned infrastructure](https://github.com/mESmaC/oia/issues/47)
- [#48: Monitor local situations and changes in public sources](https://github.com/mESmaC/oia/issues/48)
- [#49: Package supported releases and complete operational quality gates](https://github.com/mESmaC/oia/issues/49)

## Maintenance

- Update the corresponding issue when a requirement, decision, dependency, or acceptance criterion changes.
- Move Project items to In Progress when work starts, and Done only when completion evidence is recorded.
- Close phase epics only after their integrated workflow and applicable acceptance checks pass.
- Preserve dependency links and requirement identifiers when splitting a work package.
- Follow AGENTS.md for focused changes, honest validation, and safe worktree cleanup after PR merge.
