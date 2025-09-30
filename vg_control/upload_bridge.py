from .data.owl import upload_all_files
import argparse
import sys
from importlib.metadata import version


def main():
    # Set up argument parser
    parser = argparse.ArgumentParser(description="Upload Bridge")
    parser.add_argument(
        "--api-token", type=str, required=True, help="API token for upload"
    )
    parser.add_argument(
        "--progress", action="store_true", help="Enable progress output for UI"
    )

    # Parse arguments
    args = parser.parse_args()

    token = args.api_token.strip()
    progress_mode = args.progress

    print(
        f"Upload bridge v{version('vg-control')} starting with token={token[:4]}... progress={progress_mode}"
    )

    try:
        upload_all_files(token, progress_mode=progress_mode)
        print("Upload completed successfully")
        return 0
    except Exception as e:
        import traceback

        error_msg = (
            f"Error during upload: {str(e)}\n\nTraceback:\n{traceback.format_exc()}"
        )
        print(error_msg)
        with open("error.txt", "w") as f:
            f.write(error_msg)
        return 1


if __name__ == "__main__":
    sys.exit(main())
