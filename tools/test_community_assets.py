"""Community fixture helpers; run: python3 -m unittest discover -s tools -p 'test_*.py'."""
import configparser
from pathlib import Path
import tempfile
import unittest

from PIL import Image

import fetch_community_assets as fetch


class Helpers(unittest.TestCase):
    def test_clip_names_become_animation_slots(self):
        cfg = configparser.ConfigParser()
        cfg.read_string(fetch.ini_for(["AttackA.flc", "AttackB.flc", "Default.flc", "Run.flc", "Fort.flc",
                                       "Death.flc", "Victory.flc", "notes.flc"]))
        self.assertEqual(dict(cfg["Animations"]), {
            "attack1": "AttackA.flc", "attack2": "AttackB.flc", "default": "Default.flc",
            "run": "Run.flc", "fortify": "Fort.flc", "death": "Death.flc", "victory": "Victory.flc"})

    def test_paths_resolve_in_any_case(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "Art" / "Terrain").mkdir(parents=True)
            (Path(tmp) / "Art" / "Terrain" / "xPGC.PCX").write_bytes(b"")
            hit = fetch.find_ci(tmp, "art/terrain/xpgc.pcx")
            self.assertEqual(hit.name, "xPGC.PCX")
            self.assertIsNone(fetch.find_ci(tmp, "art/terrain/none.pcx"))


class Overlay(unittest.TestCase):
    def test_a_sheet_replaces_a_generated_one_only_at_the_same_size(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, src = Path(tmp) / "out", Path(tmp) / "src"
            (out / "Art" / "Terrain").mkdir(parents=True)
            src.mkdir()
            Image.new("P", (64, 32), 1).save(out / "Art" / "Terrain" / "Sheet.pcx")
            Image.new("P", (64, 32), 2).save(src / "sheet.pcx")
            Image.new("P", (32, 32), 3).save(src / "other.pcx")
            ov = fetch.Overlay(out, src)
            self.assertTrue(ov.image(src / "sheet.pcx", ("Art/Terrain/sheet.pcx",), "terrain"))
            self.assertEqual(Image.open(out / "Art" / "Terrain" / "Sheet.pcx").getpixel((0, 0)), 2)
            Image.new("P", (64, 32), 1).save(out / "Art" / "Terrain" / "Other.pcx")
            self.assertFalse(ov.image(src / "other.pcx", ("Art/Terrain/other.pcx",), "terrain"))
            self.assertIn("is not", ov.skipped[0][1])


if __name__ == "__main__":
    unittest.main()
