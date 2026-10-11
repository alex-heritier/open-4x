"""Contracts of the road and railroad sheets: the layout of the cells and that neighbours join.

Run from art/: python -m unittest discover -s tests
"""
import unittest

import numpy as np
from forge import roads
from forge.roads import CENTRE, EXIT, H, INWARD, NAMES, W, render_rail, render_road


class LayoutTests(unittest.TestCase):
    def test_cells_round_trip(self):
        self.assertEqual(len(roads.SIMPLE), 37)
        for mask in range(256):
            self.assertEqual(roads.cell_content(roads.sheet_index(mask)), (mask, 0))
        for variant in range(1, roads.ROAD_VARIANTS):
            for mask in roads.SIMPLE:
                self.assertEqual(roads.cell_content(roads.sheet_index(mask, variant)), (mask, variant))
        last = roads.sheet_index(roads.SIMPLE[-1], roads.ROAD_VARIANTS - 1)
        self.assertEqual(last, 256 + (roads.ROAD_VARIANTS - 1) * 37 - 1)

    def test_exits_pair_straightest_first(self):
        self.assertEqual(roads.plan(0b00100010), ([(1, 5)], []))  # E and W run straight through
        self.assertEqual(roads.plan(0b10101010), ([(1, 5), (3, 7)], []))  # a crossing
        self.assertEqual(roads.plan(0b00100110), ([(1, 5)], [2]))  # the line, and a branch off it
        self.assertEqual(roads.plan(0b00000110), ([], [1, 2]))  # neighbouring exits: a fork
        for mask in range(256):
            pairs, left = roads.plan(mask)
            used = sorted([i for pair in pairs for i in pair] + left)
            self.assertEqual(used, [i for i in range(8) if mask >> i & 1])


def over(top, bottom):
    a = top[..., 3:] + bottom[..., 3:] * (1 - top[..., 3:])
    rgb = (top[..., :3] * top[..., 3:] + bottom[..., :3] * bottom[..., 3:] * (1 - top[..., 3:])) / np.maximum(a, 1e-6)
    return np.concatenate([rgb, a], axis=-1)


class JoinTests(unittest.TestCase):
    """Two cells that meet draw one road across their shared line."""

    def joined(self, render, i):
        """A cell leaving by exit `i` next to the cell leaving by the opposite exit, on one canvas."""
        shift = np.rint((EXIT[i] - CENTRE) * 2).astype(int)
        origin = np.array([W, H])
        canvas = np.zeros((H * 3, W * 3, 4), dtype=np.float32)
        for cell, at in ((render(1 << ((i + 4) % 8), 2), origin + shift), (render(1 << i, 1), origin)):
            layer = np.zeros_like(canvas)
            layer[at[1] : at[1] + H, at[0] : at[0] + W] = cell
            canvas = over(layer, canvas)
        return canvas, origin

    def test_roads_join_across_every_exit(self):
        for render in (render_road, render_rail):
            for i in range(8):
                with self.subTest(render=render.__name__, exit=NAMES[i]):
                    canvas, origin = self.joined(render, i)
                    ahead = -INWARD[i]  # out of the cell, into the neighbour
                    across = np.array([ahead[1], -ahead[0]])
                    points = [origin + EXIT[i] + ahead * s for s in np.arange(-10, 10.5, 1.0)]
                    samples = np.array([canvas[int(p[1]), int(p[0])] for p in points])
                    self.assertGreater(float(samples[:, 3].min()), 0.95)  # no gap on the centreline
                    if render is render_road:  # (rails alternate sleeper and gap along the line)
                        self.assertLess(float(np.abs(np.diff(samples[:, :3], axis=0)).max()), 0.2)  # no colour jump
                    widths = []
                    for s in (-4.0, 0.0, 4.0):
                        line = [origin + EXIT[i] + ahead * s + across * t for t in np.arange(-10, 10.01, 0.25)]
                        widths.append(sum(float(canvas[int(p[1]), int(p[0]), 3]) for p in line) * 0.25)
                    self.assertLess(max(widths) - min(widths), 1.6, widths)  # one width through the join

    def test_variants_leave_a_cell_alike(self):
        # the wander fades out toward an exit, so any two variants of a cell meet a neighbour alike
        for render in (render_road, render_rail):
            cells = [render(0b00100010, v) for v in range(3)]
            for x in (0, W - 1):
                for other in cells[1:]:
                    diff = np.abs(other[H // 2 - 6 : H // 2 + 6, x, 3] - cells[0][H // 2 - 6 : H // 2 + 6, x, 3])
                    self.assertLess(float(diff.max()), 0.1, render.__name__)

    def test_nothing_is_drawn_beyond_the_exits(self):
        cell = render_road(0b00100010, 1)
        self.assertEqual(float(cell[:8, :, 3].max()), 0.0)
        self.assertEqual(float(cell[-8:, :, 3].max()), 0.0)
        self.assertGreater(float((cell[..., 3] > 0.95).sum()), 400)


if __name__ == "__main__":
    unittest.main()
