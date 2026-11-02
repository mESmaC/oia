# AI Verification Scripts

## Purpose and Role

These scripts are offline development and verification tools for testing OIA AI grounding contracts, citation bounds, prompt injection containment, and fixture integrity.
**No production runtime path calls or depends on Python or these scripts.** Production runtime domain logic, services, and algorithms belong in Rust (see `AGENTS.md` §4).

## Requirements

- Python: >= 3.10 (tested with Python 3.12.10 on Windows x86_64).
- Dependencies: Standard library only (`hashlib`, `json`, `pathlib`, `sys`, `copy`). No third-party packages required.

## Commands

Run the positive fixture and manifest checks:

```sh
python -I scripts/ai/verify_ai_contracts.py
```

Run positive checks plus the 6 negative tamper and safety mutation self-tests:

```sh
python -I scripts/ai/verify_ai_contracts.py --self-test
```
