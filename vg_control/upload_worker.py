import asyncio
import os
import tarfile
import tempfile
import uuid
import json
from typing import List, Tuple

from .constants import ROOT_DIR, RECORDING_WIDTH, RECORDING_HEIGHT, FPS
from .data.uploader import upload_archive


def find_unuploaded_sessions(root_dir: str = ROOT_DIR) -> List[Tuple[str, str, str]]:
    """Return a list of (directory, video_file, csv_file) tuples for sessions that have not been uploaded."""
    sessions = []
    for root, _, files in os.walk(root_dir):
        if ".uploaded" in files:
            continue
        mp4s = [f for f in files if f.lower().endswith(".mp4")]
        csvs = [f for f in files if f.lower().endswith(".csv")]
        if mp4s and csvs:
            sessions.append((root, mp4s[0], csvs[0]))
    return sessions


def create_tar(directory: str, video_file: str, csv_file: str) -> str:
    """Create a temporary tar archive containing the video, csv, and metadata."""
    tmp_dir = tempfile.gettempdir()
    tar_path = os.path.join(tmp_dir, f"{uuid.uuid4().hex[:16]}.tar")
    with tarfile.open(tar_path, "w") as tar:
        tar.add(os.path.join(directory, video_file), arcname=video_file)
        tar.add(os.path.join(directory, csv_file), arcname=csv_file)
        # Add metadata.json if it exists
        metadata_path = os.path.join(directory, "metadata.json")
        if os.path.exists(metadata_path):
            tar.add(metadata_path, arcname="metadata.json")
    return tar_path


def mark_uploaded(directory: str) -> None:
    """Create a .uploaded file in the session directory."""
    flag_path = os.path.join(directory, ".uploaded")
    with open(flag_path, "w"):
        pass


async def process_session(
    api_key: str, directory: str, video_file: str, csv_file: str
) -> None:
    """Compress and upload a single session."""
    tar_path = create_tar(directory, video_file, csv_file)
    try:
        # Read metadata.json to get duration
        metadata_path = os.path.join(directory, "metadata.json")
        duration = None
        if os.path.exists(metadata_path):
            try:
                with open(metadata_path) as f:
                    metadata = json.load(f)
                duration = float(metadata.get("duration", 0))
            except Exception as e:
                print(f"Warning: Could not read metadata from {metadata_path}: {e}")

        # Extract game name from directory path for tags
        dir_parts = directory.split(os.sep)
        game_name = dir_parts[-2] if len(dir_parts) > 1 else "unknown"

        upload_archive(
            api_key,
            tar_path,
            video_filename=video_file,
            control_filename=csv_file,
            tags=[game_name, os.path.basename(directory)],
            video_duration_seconds=duration,
            video_width=RECORDING_WIDTH,
            video_height=RECORDING_HEIGHT,
            video_fps=FPS,
        )
        mark_uploaded(directory)
    finally:
        if os.path.exists(tar_path):
            os.remove(tar_path)
        await asyncio.sleep(1)


async def process_all_sessions(api_key: str, root_dir: str = ROOT_DIR) -> None:
    """Find and upload all unuploaded sessions once."""
    sessions = find_unuploaded_sessions(root_dir)
    for directory, video_file, csv_file in sessions:
        try:
            await process_session(api_key, directory, video_file, csv_file)
        except Exception as exc:
            print(f"Failed to upload {directory}: {exc}")


async def upload_worker(
    api_key: str, root_dir: str = ROOT_DIR, check_interval: int = 300
) -> None:
    """Background task that periodically uploads any new sessions."""
    while True:
        await process_all_sessions(api_key, root_dir)
        await asyncio.sleep(check_interval)
