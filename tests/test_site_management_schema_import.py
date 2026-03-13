import subprocess
import sys
from pathlib import Path


def test_site_management_schema_compiles() -> None:
    schema_path = Path(__file__).resolve().parents[1] / "src" / "modules" / "site_management" / "schemas.py"

    result = subprocess.run(
        [sys.executable, "-m", "py_compile", str(schema_path)],
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 0, (
        "schemas.py should compile without syntax errors\n"
        f"stdout:\n{result.stdout}\n"
        f"stderr:\n{result.stderr}\n"
    )
