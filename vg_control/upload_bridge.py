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
        "--unreliable-connections",
        action="store_true",
        help="Tweak upload settings for unreliable connections",
    )

    # Parse arguments
    args = parser.parse_args()

    token = args.api_token.strip()

    print(f"Upload bridge v{version('vg-control')} starting with token={token[:4]}")

    try:
        upload_all_files(
            token,
            unreliable_connections=args.unreliable_connections,
        )
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
