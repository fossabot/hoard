from .hoard_tester import HoardTester, HoardFile, Environment
from pathlib import Path
import hashlib
import json
import os
import secrets
import subprocess


class OperationCheckerTester(HoardTester):
    def __init__(self):
        super().__init__()
        # Do setup
        self.reset()
        # We are not changing env on this test
        self.env = {"USE_ENV": "1"}

    def _checksum_matches(self, hoard_name, hoard_path, content, *, matches, message="", uuid=None):
        if uuid is None:
            uuid = self.uuid
        operation_log_dir = self.data_dir_path().joinpath("history", uuid, hoard_name)
        latest = None
        self.sync()
        print("##  Scanning for Operation Logs")
        with os.scandir(operation_log_dir) as it:
            for entry in it:
                print(f"### Found {entry.path}")
                is_later = latest is None or Path(latest.path).name < Path(entry.path).name
                if entry.is_file() and "last_paths" not in entry.name and is_later:
                    print("    Marking as latest entry")
                    latest = entry
        with open(latest.path) as file:
            op_json = json.load(file)
            for item in hoard_path:
                op_json = op_json[item]

        md5 = hashlib.md5(content).hexdigest()

        if matches:
            assert md5 == op_json, f"expected file hash {md5} to match logged checksum {op_json} for uuid {uuid}: {message}"
        else:
            assert md5 != op_json, f"expected file hash {md5} to NOT match logged checksum {op_json} for uuid {uuid}: {message}"

    def _assert_anon_file_checksum_matching(self, content, *, matches, message="", uuid=None):
        self._checksum_matches(
            "anon_file", ["hoard", "Anonymous", ""], content,
            matches=matches, message=message, uuid=uuid
        )

    def _run1(self):
        # Run hoard
        print("========= HOARD RUN #1 =========")
        self.run_hoard("backup")

    def _run2(self):
        # Read UUID and delete file
        self.old_uuid = self.uuid
        os.remove(self.get_uuid_path())
        # Go again, this time with a different uuid
        # This should still succeed because the files have the same checksum
        print("========= HOARD RUN #2 =========")
        print("  After removing the UUID file  ")
        self.run_hoard("backup")
        assert self.uuid != self.old_uuid, "a new UUID should have been generated"

    def _run3(self):
        # Modify a file and backup again so checksums are different the next time
        # This should succeed because this UUID had the last backup
        self.old_content = self.read_hoard_file(Environment.First, HoardFile.AnonFile)
        self._assert_anon_file_checksum_matching(
            self.old_content, matches=True, message="last checksum should match old data"
        )

        new_content = secrets.token_bytes(1024)
        assert new_content != self.old_content, "new content should differ from old"
        self.write_hoard_file(Environment.First, HoardFile.AnonFile, new_content)
        assert new_content == self.read_hoard_file(Environment.First, HoardFile.AnonFile), "file should contain new, different content"
        self._assert_anon_file_checksum_matching(
            new_content, matches=False, message="new data should not match old checksum"
        )

        print("========= HOARD RUN #3 =========")
        print(" After replacing a file content ")
        self.run_hoard("backup")

        self._assert_anon_file_checksum_matching(
            self.old_content, matches=False, message="new last checksum should no longer match old data"
        )
        self._assert_anon_file_checksum_matching(
            new_content, matches=True, message="new last checksum should match new data"
        )

    def _run4(self):
        # Swap UUIDs and change the file again and try to back up
        # Should fail because a different machine has the most recent backup
        new_uuid = self.uuid
        assert self.old_uuid != self.uuid, "new UUID should not match old one"
        self.uuid = self.old_uuid
        assert self.uuid == self.old_uuid, "UUID should now be set to old one"
        self._assert_anon_file_checksum_matching(
            self.old_content, matches=False, uuid=new_uuid, message="old data should not match latest checksum (from newer UUID)"
        )

        self.write_hoard_file(Environment.First, HoardFile.AnonFile, self.old_content)
        assert self.old_content == self.read_hoard_file(Environment.First, HoardFile.AnonFile), "file should now contain the old content"
        # Should already be False, but making sure.
        self.force = False

        try:
            print("========= HOARD RUN #4 =========")
            print("   After using first UUID/File  ")
            self.run_hoard("backup")
            raise AssertionError("Using the first UUID should have failed (1)")
        except subprocess.CalledProcessError:
            pass

        # Once more to verify it should always fail
        try:
            print("========= HOARD RUN #5 =========")
            print("    Doing it again to be sure   ")
            self.run_hoard("backup")
            raise AssertionError("Using the first UUID should have failed (2)")
        except subprocess.CalledProcessError:
            pass


    def _run5(self):
        # Do it again but forced, and it should succeed
        print("========= HOARD RUN #6 =========")
        print("    Doing it again to be sure   ")
        self.force = True
        self.run_hoard("backup")
        self._assert_anon_file_checksum_matching(self.old_content, matches=True)

    def run_test(self):
        self._run1()
        self._run2()
        self._run3()
        self._run4()
        self._run5()
