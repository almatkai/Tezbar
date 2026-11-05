import json,pathlib,tempfile,unittest
from unittest.mock import patch
import build_identity as identity
class BuildIdentityTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.binary=pathlib.Path(self.tmp.name)/'tezbar';self.binary.write_bytes(b'exact built binary')
        self.receipt=dict(source='a'*40,dirty=False,binary_sha256=identity.sha(self.binary),rustc_commit='c'*40,resource_files={'resource':'hash'})
    def write(self):self.binary.with_name('tezbar.build-receipt.json').write_text(json.dumps(self.receipt))
    def validate(self):
        self.write()
        with patch.object(identity,'compiler',return_value='c'*40),patch.object(identity,'resources',return_value={'resource':'hash'}):return identity.baseline_version(self.binary,'a'*40)
    def test_accept_exact_clean_build(self):self.assertEqual(self.validate(),self.receipt)
    def test_reject_older_source(self):
        self.receipt['source']='b'*40
        with self.assertRaises(RuntimeError):self.validate()
    def test_reject_replaced_binary(self):
        self.binary.write_bytes(b'new binary')
        with self.assertRaises(RuntimeError):self.validate()
    def test_reject_dirty_build(self):
        self.receipt['dirty']=True
        with self.assertRaises(RuntimeError):self.validate()
    def test_reject_compiler_drift(self):
        self.receipt['rustc_commit']='d'*40
        with self.assertRaises(RuntimeError):self.validate()
    def test_reject_changed_resources(self):
        self.receipt['resource_files']={}
        with self.assertRaises(RuntimeError):self.validate()
    def test_compiler_is_verified_from_binary_not_receipt(self):
        self.binary.write_bytes(b'/rustc/'+b'c'*40+b'/library/test')
        with patch.object(identity,'expected_compiler',return_value='c'*40):self.assertEqual(identity.compiler(self.binary),'c'*40)
        with patch.object(identity,'expected_compiler',return_value='d'*40),self.assertRaises(RuntimeError):identity.compiler(self.binary)
if __name__=='__main__':unittest.main()
