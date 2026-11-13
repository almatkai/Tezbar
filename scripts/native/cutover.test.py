"""Private byte fixtures for the review-only cutover generator; no product edits."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('cutover', Path(__file__).resolve().parents[2]/'docs/native-m11/integration/prepare-cutover.py')
cutover = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cutover)

class CutoverBytes(unittest.TestCase):
    def test_empty_relocation_preserves_existence_and_deletes_source(self):
        with tempfile.TemporaryDirectory(prefix='tezbar-cutover-empty-') as temp:
            root=Path(temp)
            (root/'empty.txt').touch()
            changes={'empty.txt':None, 'archive/empty.txt':''}
            patch=cutover.make_patch(changes,root)
            self.assertIn(b'new file mode 100644',patch)
            proof=cutover.verify_roundtrip(changes,patch,root)
            self.assertEqual(proof['files'],2)
            self.assertTrue((root/'empty.txt').exists())

    def test_relocation_keeps_executable_mode(self):
        with tempfile.TemporaryDirectory(prefix='tezbar-cutover-mode-') as temp:
            root=Path(temp)
            source=root/'bundle.sh'
            source.write_text('#!/bin/sh\nexit 0\n')
            source.chmod(0o755)
            changes={'bundle.sh':None, 'archive/bundle.sh':source.read_text()}
            modes={'archive/bundle.sh':cutover.file_mode(source)}
            patch=cutover.make_patch(changes,root,modes)
            self.assertIn(b'new file mode 100755',patch)
            cutover.verify_roundtrip(changes,patch,root,modes)

    def test_lf_only_preserves_all_other_line_separators_crlf_and_missing_newline(self):
        separators='\v\f\x1c\x1d\x1e\x85\u2028\u2029'
        with tempfile.TemporaryDirectory(prefix='tezbar-cutover-unicode-') as temp:
            root=Path(temp)
            before='first\r\nkeep '+separators+' together\r\nlast without newline'
            (root/'fixture.txt').write_bytes(before.encode())
            new=before.replace('first','changed').replace('last','final')
            changes={'fixture.txt':new,'copy.txt':before,'new.txt':'new\x85value\n'}
            patch=cutover.make_patch(changes,root)
            proof=cutover.verify_roundtrip(changes,patch,root)
            self.assertEqual(proof['files'],3)
            self.assertEqual(''.join(cutover.lf_lines(before)),before)
            self.assertEqual(len(cutover.lf_lines(before)),3)
            self.assertEqual((root/'fixture.txt').read_bytes(),before.encode())

if __name__ == '__main__':
    unittest.main()
