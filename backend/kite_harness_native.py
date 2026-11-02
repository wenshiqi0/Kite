"""Native adapters share one capability contract; provider protocols stay private."""
import os
from kite_rpc import LIMIT


def Native(provider, command, cwd, model, event=lambda value: None, permission=lambda value: None,
           saved=None, persist=lambda value: None, inspect=False, route=None):
    if provider == "dsh":
        from kite_dsh_acp import Dsh
        return Dsh(command, cwd, route or os.environ.get("KITE_DSH_PROVIDER", "deepseek-official"),
                   model, event, permission, saved, persist, inspect)
    if provider == "ply":
        from kite_ply import Ply
        return Ply(command, cwd, model, event, permission, saved, persist, inspect)
    raise ValueError("unsupported native harness")
