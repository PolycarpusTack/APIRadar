"""Packaging sanity tests — the SDK must actually be installable.

Regression tests for O-9-T1: a nonexistent build backend and a Django extra
with no Django integration shipped both made `pip install` dishonest.
"""
from __future__ import annotations

import importlib
import re
import unittest
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except ImportError:  # pragma: no cover - Python 3.9/3.10 fallback
    tomllib = None

SDK_ROOT = Path(__file__).resolve().parent.parent
PYPROJECT = SDK_ROOT / "pyproject.toml"


def _load_pyproject() -> dict:
    if tomllib is not None:
        with open(PYPROJECT, "rb") as f:
            return tomllib.load(f)
    raise unittest.SkipTest("tomllib unavailable on this Python")


class TestBuildBackend(unittest.TestCase):
    def test_declared_build_backend_exists(self):
        """The declared build backend must be importable, or pip install fails."""
        data = _load_pyproject()
        backend = data["build-system"]["build-backend"]
        module_name, _, attr = backend.partition(":")
        module = importlib.import_module(module_name)
        if attr:
            obj = module
            for part in attr.split("."):
                obj = getattr(obj, part)
        # A PEP 517 backend must expose the mandatory hooks.
        target = module if not attr else obj
        self.assertTrue(hasattr(target, "build_wheel"),
                        f"backend {backend!r} lacks the mandatory build_wheel hook")
        self.assertTrue(hasattr(target, "build_sdist"),
                        f"backend {backend!r} lacks the mandatory build_sdist hook")


class TestExtrasAreHonest(unittest.TestCase):
    def test_no_extra_without_shipped_integration(self):
        """Every declared extra must correspond to code the package ships.

        There is no Django middleware in radar_monitor, so a `django` extra
        would advertise an integration that does not exist.
        """
        data = _load_pyproject()
        extras = set(data.get("project", {}).get("optional-dependencies", {}))
        self.assertEqual(extras, {"fastapi"},
                         "declared extras must match shipped integrations "
                         "(only ASGI/Starlette middleware exists)")


class TestPython39Compatibility(unittest.TestCase):
    def test_pep604_unions_require_future_annotations(self):
        """requires-python is >=3.9: any file using `X | Y` annotations must
        defer annotation evaluation with `from __future__ import annotations`,
        or import/collection raises TypeError on 3.9."""
        union_pattern = re.compile(r":\s*[\w.\[\]]+\s*\|\s*[\w.\[\]]+")
        offenders = []
        for py_file in list((SDK_ROOT / "radar_monitor").glob("*.py")) + list(
            (SDK_ROOT / "tests").glob("*.py")
        ):
            source = py_file.read_text(encoding="utf-8")
            if union_pattern.search(source) and \
                    "from __future__ import annotations" not in source:
                offenders.append(py_file.name)
        self.assertEqual(offenders, [],
                         f"files use PEP 604 unions without the future import: {offenders}")


if __name__ == "__main__":
    unittest.main()
