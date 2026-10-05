"""Cache provenance tests; run: python3 -m unittest discover -s tools -p 'test_*.py'."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import prep_assets as prep


class CacheRoots(unittest.TestCase):
    def test_identical_sources_in_different_roots_are_stale(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b, cache = [Path(tmp)/n for n in ('a','b','cache')]
            for root in (a, b):
                root.mkdir()
                (root/'source').write_bytes(b'flat colour')
                os.utime(root/'source', (1000, 1000))
            cache.mkdir()
            (cache/'picture').write_bytes(b'converted')
            with patch.object(prep, 'GOG', str(a)), patch.object(prep, 'OUT', str(cache)):
                entry = {'root': os.path.realpath(a), 'outputs': ['picture'],
                         'sources': [prep.stat_of('source')], 'params': None, 'search': []}
                index = {'entries': {'stage:test': entry}}
                self.assertTrue(prep.fresh(index, 'stage:test', search_dependent=True))
                with patch.object(prep, 'GOG', str(b)):
                    self.assertFalse(prep.fresh(index, 'stage:test', search_dependent=True))
                del entry['root']
                self.assertFalse(prep.fresh(index, 'stage:test'))

    def test_intermediate_reads_keep_original_sources(self):
        with tempfile.TemporaryDirectory() as tmp:
            root, cache = Path(tmp)/'root', Path(tmp)/'cache'
            root.mkdir()
            cache.mkdir()
            (root/'source').write_bytes(b'flat colour')
            (cache/'picture').write_bytes(b'converted')
            with patch.object(prep, 'GOG', str(root)), patch.object(prep, 'OUT', str(cache)), \
                 patch.object(prep, '_READ', set()), \
                 patch.object(prep, '_GENERATED_SOURCES', {str(cache/'picture'): ['source']}):
                prep.track(cache/'picture')
                self.assertEqual(prep._READ, {'source'})


if __name__ == '__main__':
    unittest.main()
