"""Independent geometry/occlusion checks and contracts for the shipped surface art.

Run from art/: python -m unittest discover -s tests
"""
import json
import unittest

import numpy as np
from PIL import Image
from forge import relief
from forge.util import PACK


class ReliefTests(unittest.TestCase):
    def test_all_contexts_have_tall_art_and_matching_four_edge_sheets(self):
        visuals = json.loads((PACK / 'pack.json').read_text())['visuals']
        self.assertEqual(len(visuals['relief_borders']), 18)
        for name, path in visuals['relief_borders'].items():
            with self.subTest(name=name):
                art_path = visuals['mountain'] if name == 'mountain' else visuals['overlays'][name]
                with Image.open(PACK / art_path) as art, Image.open(PACK / path) as sheet:
                    self.assertEqual(art.size, (256, 224))
                    self.assertEqual(sheet.size, (1024, 224))
                    self.assertEqual(sheet.mode, 'RGBA')
                    alpha = np.asarray(sheet.getchannel('A'))
                    self.assertFalse(alpha[:3].any(), 'no clipping into the top of the cell')
                    self.assertLessEqual(int(alpha[-3:].max()), 3, 'only antialiasing may reach the cell bottom')
                    for edge in range(4):
                        self.assertGreater(np.count_nonzero(alpha[:, edge*256:(edge+1)*256]), 10)
                if name.startswith('mountain'):
                    # The tile's northern apex is y=96, so these pixels overlap the tile behind.
                    with Image.open(PACK / art_path) as art:
                        self.assertGreater(np.count_nonzero(np.asarray(art.getchannel('A'))[:96]), 100)

    def test_foreground_surface_hides_rear_paint_even_when_its_mask_is_transparent(self):
        # An analytic wall at the front must occlude a painted rear surface. This tests the
        # compositing behavior without reproducing the procedural mountain generation.
        original_grid, original_ss = relief.GRID, relief.SS
        try:
            relief.GRID, relief.SS = 64, 1
            u, v = np.meshgrid(np.linspace(0, 1, 64), np.linspace(0, 1, 64), indexing='ij')
            alpha = ((u + v) < 0.5).astype(np.float32)
            rgb = np.ones((64, 64, 3), dtype=np.float32)
            flat = relief.project(np.zeros((64, 64)), rgb, alpha)
            wall = np.where((u+v > 0.85) & (u+v < 1.2), 100, 0).astype(np.float32)
            hidden = relief.project(wall, rgb, alpha)
            self.assertGreater(flat[..., 3].sum(), 100)
            self.assertLess(hidden[..., 3].sum(), flat[..., 3].sum() * 0.1)
        finally:
            relief.GRID, relief.SS = original_grid, original_ss

    def test_cover_contexts_change_the_surface_instead_of_recoloring_flat_art(self):
        for kind, seed in [('mountain', 3), ('hills', 21)]:
            bare = relief.scene(kind, seed, 'temperate')
            forest = relief.scene(kind, seed, 'temperate', 'forest')
            jungle = relief.scene(kind, seed, 'temperate', 'jungle')
            self.assertGreater(np.count_nonzero(forest[0] > bare[0] + 2), 1000)
            self.assertGreater(np.count_nonzero(jungle[0] > bare[0] + 2), 1000)
            self.assertGreater(np.mean(np.abs(forest[3] - jungle[3])), 0.005)
            # Context changes do not move the footprint or alter the rock outside the trees.
            np.testing.assert_array_equal(bare[1], forest[1])
            np.testing.assert_array_equal(bare[2], jungle[2])


if __name__ == '__main__':
    unittest.main()
