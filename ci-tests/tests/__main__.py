import hashlib
import os
import subprocess
import sys
from pathlib import Path
from testers.ignore_filter import IgnoreFilterTester
from testers.last_paths import LastPathsTester
from testers.operations import OperationCheckerTester
from testers.cleanup import LogCleanupTester
from testers.hoard_tester import HoardFile, Environment
from testers.correct_errors import CorrectErrorsTester
from testers.no_config_dir import MissingConfigDirTester
from testers.yaml_support import YAMLSupportTester


for var in ["CI", "GITHUB_ACTIONS"]:
    val = os.environ.get(var)
    if val is None or val != "true":
        raise RuntimeError("These tests must be run on GitHub Actions!")


def print_logs():
    print("\n### Logs:")
    data_dir = LastPathsTester.data_dir_path()
    for dirpath, _, filenames in os.walk(data_dir):
        for file in filenames:
            if file.endswith(".log"):
                path = str(Path(dirpath).joinpath(file))
                print(f"\n##########\n\t{path}")
                sys.stdout.flush()
                subprocess.run(["cat", path])
                sys.stdout.flush()
                print("\n##########")


def print_checksums():
    print("\n### Checksums:")
    for env in list(Environment):
        for file in list(HoardFile):
            if file is not HoardFile.AnonDir and file is not HoardFile.NamedDir1 and file is not HoardFile.NamedDir2:
                path = Path.home().joinpath(f"{env.value}_{file.value}")
                with open(path, "rb") as file:
                    print(f"{path}: {hashlib.md5(file.read()).hexdigest()}")


if __name__ == "__main__":
    if len(sys.argv) == 1:
        raise RuntimeError("One argument - the test - is required")
    try:
        if sys.argv[1] == "last_paths":
            print("Running last_paths test")
            LastPathsTester().run_test()
        elif sys.argv[1] == "operation":
            print("Running operation test")
            OperationCheckerTester().run_test()
        elif sys.argv[1] == "ignore":
            print("Running ignore filter test")
            IgnoreFilterTester().run_test()
        elif sys.argv[1] == "cleanup":
            print("Running cleanup test")
            LogCleanupTester().run_test()
        elif sys.argv[1] == "errors":
            print("Running errors test")
            CorrectErrorsTester().run_test()
        elif sys.argv[1] == "missing_config":
            print("Running missing config dir test")
            MissingConfigDirTester().run_test()
        elif sys.argv[1] == "yaml":
            print("Running YAML compat tests")
            YAMLSupportTester().run_test()
        elif sys.argv[1] == "all":
            print("Running all tests")
            YAMLSupportTester().run_test()
            MissingConfigDirTester().run_test()
            CorrectErrorsTester().run_test()
            LastPathsTester().run_test()
            IgnoreFilterTester().run_test()
            OperationCheckerTester().run_test()
            LogCleanupTester().run_test()
        else:
            raise RuntimeError(f"Invalid argument {sys.argv[1]}")
    except Exception:
        data_dir = LastPathsTester.data_dir_path()
        print("\n### Hoards:")
        sys.stdout.flush()
        subprocess.run(["tree", str(data_dir)])
        print("\n### Home:")
        sys.stdout.flush()
        subprocess.run(["tree", "-aL", "3", str(Path.home())])
        print_checksums()
        print_logs()
        print("\n### Configs:")
        config_dir = LastPathsTester.config_file_path().parent
        for file in os.listdir(config_dir):
            file_path = config_dir.joinpath(file)
            if file_path.is_file():
                with open(file_path, "r", encoding="utf-8") as opened:
                    print(f"##### {file_path}\n")
                    print(opened.read())
        raise
