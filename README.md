<img src="assets/icon.svg" width="64" height="64" alt="">

# OIA — Open Intelligence Agency

OIA is a research project and a suite of intelligence tools built around capabilities generally available to the public. It aims to help people understand what is happening around them, look out for themselves, and make informed decisions about their privacy.

As the open-source ecosystem grows, powerful tools and information sources become accessible to more people. OIA explores what those capabilities make possible, brings them together in approachable tools, and makes their implications easier to understand.

## Purpose

- **Public awareness:** Help people discover and understand publicly available information relevant to their surroundings.
- **Individual privacy:** Demonstrate what information and capabilities are already accessible, so people can better understand and reduce their own exposure.
- **Research and innovation:** Explore public intelligence capabilities, document their strengths and limitations, and encourage improvements through open-source collaboration.

The research is part of the product: OIA should explain where information comes from, what a tool can establish, and where uncertainty remains. Public availability alone does not establish accuracy, and conclusions should remain distinguishable from observations.

## Technical direction

OIA will be implemented in **Rust**, with **Lean** used to formally specify and prove selected algorithmic properties. The intent is to use those proofs to validate reasoning and guide improvements to the Rust implementations.

Lean is a development and research tool; it does not necessarily need to be included in the distributed Rust binaries.

A proof about a Lean model does not automatically prove that a corresponding Rust implementation is correct. For algorithms supported by proofs, the project should document:

- The specification, assumptions, and properties proved in Lean.
- How the Rust implementation corresponds to that specification.
- The tests or other checks that support that correspondence, and any remaining gaps.

Verification claims will be limited to the properties and implementation connections actually established. Algorithmic proofs do not establish the accuracy of external data.

## Current status

OIA is at the project-definition stage. This repository currently records the vision; tools, Rust implementations, and Lean proofs have not yet been added.

The version 0.1 specification draft consolidates the product scope and a proposed implementation plan:

- [Product specification](docs/spec.md): workflows, visual identity, and functional requirements across the suite.
- [Architecture and data contracts](docs/architecture.md): Rust services, evidence and time models, integrations, storage, and the Lean verification boundary.
- [Delivery and verification](docs/delivery.md): proposed milestones, acceptance scenarios, release gates, and open decisions.
- [GitHub implementation backlog](docs/backlog.md): the Project, phase epics, and linked work packages covering the specification.
- [Selective reuse inventory](docs/prior-art.md): implementation groundwork in God's Eye View that can inform the Rust implementation.

The [earlier exploration draft](docs/spec-draft.md) is retained as design history. The specification describes intended behavior; implementation choices and delivery sequencing remain proposals where identified.

Coding agents should read [AGENTS.md](AGENTS.md) before making changes.
