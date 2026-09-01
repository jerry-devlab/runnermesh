#!/usr/bin/env python3
"""Small, deterministic CI job policy shared by workflow-facing tooling."""

from __future__ import annotations

import enum
from dataclasses import dataclass


class CiEvent(str, enum.Enum):
    PULL_REQUEST = "pull_request"
    PUSH = "push"
    WORKFLOW_DISPATCH = "workflow_dispatch"


@dataclass(frozen=True)
class CiDecision:
    event: CiEvent
    change_class: str
    code_ci_required: bool


def decide_ci_jobs(event_name: str, change_class: object) -> CiDecision:
    """Return the code-job decision for one supported CI event.

    Documentation-only changes use the lightweight path for pull requests and
    main pushes. Manual dispatches always run the full code jobs because their
    single-commit fallback is not a trustworthy branch-delta classification.
    """

    event = CiEvent(event_name)
    value = str(getattr(change_class, "value", change_class))
    if value not in {"DOCS_ONLY", "RUST_OR_RUNTIME_CHANGE"}:
        raise ValueError(f"unsupported change class: {value}")
    return CiDecision(
        event=event,
        change_class=value,
        code_ci_required=(
            value == "RUST_OR_RUNTIME_CHANGE"
            or event is CiEvent.WORKFLOW_DISPATCH
        ),
    )
