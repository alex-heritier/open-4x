"""The national layer is authored geometry, independent of the rest of the paint."""
import sys
import unittest
from pathlib import Path

import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from forge import cargo, fleet, troops
from forge.paint import render_unit_pair, tint_accent
from forge.rig import Scene


class UnitArtTests(unittest.TestCase):
    def test_occluding_equipment_blocks_nation_paint(self):
        sc = Scene(128, 128, (64, 70))
        sc.poly([(-20, 0, 0), (20, 0, 0), (20, 0, 30), (-20, 0, 30)], '#ffffff', None, two=True, accent=True)
        sc.poly([(-8, -1, 5), (8, -1, 5), (8, -1, 25), (-8, -1, 25)], '#805a40', None, two=True)
        base, accent = render_unit_pair(sc.render(), 128, 128, ss=2, rim=0)
        mask = np.asarray(accent)[..., 3]
        self.assertLess(mask[55, 64], 5, 'wood in front must hide the cloth behind it')
        self.assertGreater(mask[55, 48], 200)
        self.assertEqual(mask[90, 64], 0)
        red = np.asarray(tint_accent(base, accent, '#d82f28'))
        blue = np.asarray(tint_accent(base, accent, '#286ac8'))
        np.testing.assert_array_equal(red[mask == 0], blue[mask == 0])
        self.assertFalse(np.array_equal(red[55, 48], blue[55, 48]))

    def test_dropped_rifle_starts_where_the_idle_rifle_is_held(self):
        standing = troops.soldier_pose('idle', 0)
        falling = troops.soldier_pose('death', 0)
        self.assertEqual(falling.dropped[0], standing.tool[0])
        direction = np.asarray(standing.tool[1])
        np.testing.assert_allclose(falling.dropped[1], direction / np.linalg.norm(direction))

    def test_every_design_has_shaded_national_cloth(self):
        for name, build in {**troops.BUILDERS, **fleet.BUILDERS, **cargo.BUILDERS}.items():
            with self.subTest(design=name):
                size = (256, 192) if name in fleet.BUILDERS else (160, 160)
                base, accent = render_unit_pair(build(0, 'idle', 0), *size, ss=1)
                a, b = np.asarray(accent), np.asarray(base)
                self.assertGreater(a[..., 3].sum(), 200)
                np.testing.assert_array_equal(a[..., 0], a[..., 1])
                np.testing.assert_array_equal(a[..., 1], a[..., 2])
                self.assertTrue(np.all(a[..., 3] <= b[..., 3]))


if __name__ == '__main__':
    unittest.main()
