"""Required resource lifecycle contract for Kite-managed harness adapters.

The host owns archive policy. Each adapter proves that its context can be
restored, serializes release against input/restore, and releases its own runtime.
Neither discovery nor release may start a model turn or delete history.
"""
from pathlib import Path
from typing import Protocol


class HarnessLifecycle(Protocol):
    def snapshot(self, record: dict, agents=None) -> dict:
        """Return owner identity, idle/persisted facts and an activity fingerprint."""
        ...

    def owner_lock(self, record: dict) -> Path:
        """Return the lease shared with input and owner restoration."""
        ...

    def shares_owner(self, record: dict, owner: dict, other: dict) -> bool:
        """Identify another durable reference served by the same runtime."""
        ...

    def release(self, record: dict, expected: dict) -> dict:
        """Revalidate and release resources; return reclaimed/reason facts."""
        ...


def adapters(codex: HarnessLifecycle) -> dict[str, HarnessLifecycle]:
    from kite_harness import ManagedLifecycle
    managed = ManagedLifecycle()
    return {"codex": codex, "dsh": managed, "ply": managed}
