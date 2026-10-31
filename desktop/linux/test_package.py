import importlib.util,io,json,tarfile,tempfile,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('host_package',Path(__file__).with_name('package.py'));module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class PackageTests(unittest.TestCase):
    def test_archive_is_reproducible_and_contains_only_declared_distribution_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);binary=root/'binary';binary.write_bytes(b'controlled binary')
            a=root/'a.tar.gz';b=root/'b.tar.gz'
            module.package(binary,a,'0.2.0-alpha.17','a'*40);module.package(binary,b,'0.2.0-alpha.17','a'*40)
            self.assertEqual(a.read_bytes(),b.read_bytes())
            with tarfile.open(a) as archive:
                names=[Path(m.name).name for m in archive.getmembers()]
                self.assertEqual(set(names),{'kite-node','host-manifest.json','desktop.py','install.py','strings.json','LICENSE',*[f'kite-{state}.png' for state in ['ready','degraded','offline','busy']]})
                manifest=json.load(archive.extractfile(next(m for m in archive.getmembers() if m.name.endswith('/host-manifest.json'))))
                self.assertEqual(manifest['sourceCommit'],'a'*40);self.assertEqual(manifest['nodeApi'],1)
                self.assertTrue(all(m.uid==0 and m.mtime==0 for m in archive.getmembers()))
if __name__=='__main__':unittest.main()
