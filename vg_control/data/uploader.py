import os
from typing import List, Optional
from datetime import datetime
import requests
import subprocess
import shlex
from tqdm import tqdm
import time
import hashlib
import tempfile
import json

from ..constants import API_BASE_URL


def upload_archive(
    api_key: str,
    archive_path: str,
    tags: Optional[List[str]] = None,
    base_url: str = API_BASE_URL,
    progress_mode: bool = False,
    video_filename: Optional[str] = None,
    control_filename: Optional[str] = None,
    video_duration_seconds: Optional[float] = None,
    video_width: Optional[int] = None,
    video_height: Optional[int] = None,
    video_codec: Optional[str] = None,
    video_fps: Optional[float] = None,
) -> None:
    """Upload an archive to the storage bucket using multipart upload for reliability and progress tracking."""

    # Get file size
    file_size = os.path.getsize(archive_path)

    # Initialize multipart upload - server will determine chunk size
    upload_session = init_multipart_upload(
        api_key=api_key,
        archive_path=archive_path,
        total_size_bytes=file_size,
        tags=tags,
        base_url=base_url,
        video_filename=video_filename,
        control_filename=control_filename,
        video_duration_seconds=video_duration_seconds,
        video_width=video_width,
        video_height=video_height,
        video_codec=video_codec,
        video_fps=video_fps,
    )

    upload_id = upload_session["upload_id"]
    game_control_id = upload_session["game_control_id"]
    total_chunks = upload_session["total_chunks"]
    chunk_size_bytes = upload_session["chunk_size_bytes"]

    print(
        f"Starting upload: {file_size} bytes in {total_chunks} chunks of {chunk_size_bytes} bytes each"
    )
    print(f"Upload initiated: upload_id={upload_id}, game_control_id={game_control_id}")

    # Track upload success for cleanup
    upload_success = False

    try:
        # Initialize progress tracking
        if progress_mode:
            progress_file = os.path.join(
                tempfile.gettempdir(), "owl-control-upload-progress.json"
            )
            initial_progress = {
                "phase": "upload",
                "action": "start",
                "bytes_uploaded": 0,
                "total_bytes": file_size,
                "percent": 0,
                "speed_mbps": 0,
                "eta_seconds": 0,
                "timestamp": time.time(),
                "current_chunk": 0,
                "total_chunks": total_chunks,
            }
            try:
                with open(progress_file, "w") as f:
                    json.dump(initial_progress, f)
            except Exception as e:
                print(f"Warning: Could not initialize progress file: {e}")

        def emit_upload_progress(
            bytes_uploaded, total_bytes, current_chunk, speed_bps=0
        ):
            """Write JSON progress data to file for UI consumption"""
            if progress_mode:
                progress_data = {
                    "phase": "upload",
                    "action": "progress",
                    "bytes_uploaded": bytes_uploaded,
                    "total_bytes": total_bytes,
                    "percent": min((bytes_uploaded / total_bytes) * 100, 100)
                    if total_bytes > 0
                    else 0,
                    "speed_mbps": speed_bps / (1024 * 1024) if speed_bps > 0 else 0,
                    "eta_seconds": ((total_bytes - bytes_uploaded) / speed_bps)
                    if speed_bps > 0
                    else 0,
                    "timestamp": time.time(),
                    "current_chunk": current_chunk,
                    "total_chunks": total_chunks,
                }

                progress_file = os.path.join(
                    tempfile.gettempdir(), "owl-control-upload-progress.json"
                )
                try:
                    with open(progress_file, "w") as f:
                        json.dump(progress_data, f)
                except Exception as e:
                    print(f"Warning: Could not write progress file: {e}")

                print(f"PROGRESS: {json.dumps(progress_data)}")

        # Upload chunks
        chunk_etags = []
        bytes_uploaded = 0
        start_time = time.time()

        with open(archive_path, "rb") as f:
            with tqdm(
                total=file_size, unit="B", unit_scale=True, desc="Uploading"
            ) as pbar:
                for chunk_num in range(1, total_chunks + 1):
                    # Read chunk data
                    chunk_data = f.read(chunk_size_bytes)
                    if not chunk_data:
                        break

                    # Upload chunk using the dedicated function
                    try:
                        etag = upload_single_chunk(
                            base_url=base_url,
                            api_key=api_key,
                            upload_id=upload_id,
                            chunk_data=chunk_data,
                            chunk_number=chunk_num,
                        )

                        chunk_etags.append(
                            {
                                "chunk_number": chunk_num,
                                "etag": etag,
                            }
                        )

                        # Update progress
                        bytes_uploaded += len(chunk_data)
                        pbar.update(len(chunk_data))

                        # Calculate speed and emit progress
                        if progress_mode:
                            elapsed_time = time.time() - start_time
                            speed_bps = (
                                bytes_uploaded / elapsed_time if elapsed_time > 0 else 0
                            )
                            emit_upload_progress(
                                bytes_uploaded, file_size, chunk_num, speed_bps
                            )

                        print(
                            f"Uploaded chunk {chunk_num}/{total_chunks} ({len(chunk_data)} bytes, ETag: {etag})"
                        )

                    except Exception as e:
                        print(f"Failed to upload chunk {chunk_num}: {e}")
                        raise

        # Completing upload
        print(f"Completing upload with {len(chunk_etags)} chunks...")
        completion_result = complete_multipart_upload(
            api_key=api_key,
            upload_id=upload_id,
            chunk_etags=chunk_etags,
            base_url=base_url,
        )

        # Check if completion succeeded
        if completion_result.get("success", True):
            print("Upload completed successfully!")
        else:
            error_msg = completion_result.get("message", "Unknown error")
            raise Exception(f"Upload failed after all attempts: {error_msg}")

        # Write final progress
        if progress_mode:
            progress_file = os.path.join(
                tempfile.gettempdir(), "owl-control-upload-progress.json"
            )
            try:
                final_progress = {
                    "phase": "upload",
                    "action": "complete",
                    "bytes_uploaded": file_size,
                    "total_bytes": file_size,
                    "percent": 100,
                    "speed_mbps": 0,
                    "eta_seconds": 0,
                    "timestamp": time.time(),
                    "current_chunk": total_chunks,
                    "total_chunks": total_chunks,
                    "game_control_id": completion_result["game_control_id"],
                    "verified": completion_result.get("verified", False),
                }
                with open(progress_file, "w") as f:
                    json.dump(final_progress, f)
            except Exception as e:
                print(f"Warning: Could not write final progress: {e}")

            print("Upload completed successfully!")
            print(f"Game Control ID: {completion_result['game_control_id']}")
            print(f"Object Key: {completion_result['object_key']}")
            print(f"Verified: {completion_result.get('verified', False)}")

            # Mark upload as successful
            upload_success = True

    finally:
        # If upload failed for any reason, abort the multipart upload
        if not upload_success:
            print("Aborting multipart upload due to upload failure...")
            try:
                abort_multipart_upload(
                    api_key=api_key, upload_id=upload_id, base_url=base_url
                )
                print("Multipart upload aborted successfully")
            except Exception as abort_error:
                print(f"Warning: Failed to abort multipart upload: {abort_error}")


def upload_single_chunk(
    api_key: str,
    upload_id: str,
    base_url: str,
    chunk_data: bytes,
    chunk_number: int,
    max_retries: int = 5,
) -> str:
    """
    Upload a single chunk using curl with retry logic.

    Args:
        chunk_data: The chunk data to upload
        chunk_number: The chunk number (for logging)
        max_retries: Maximum number of retry attempts

    Returns:
        str: The ETag from the successful upload

    Raises:
        Exception: If all retry attempts fail
    """
    # Calculate chunk hash
    chunk_hash = hashlib.sha256(chunk_data).hexdigest()

    with tempfile.NamedTemporaryFile(delete=False) as temp_chunk_file:
        temp_chunk_file.write(chunk_data)
        temp_chunk_path = temp_chunk_file.name

    try:
        for retry in range(max_retries):
            try:
                # Get upload URL for this chunk
                chunk_upload_url = get_multipart_chunk_upload_url(
                    api_key=api_key,
                    upload_id=upload_id,
                    chunk_number=chunk_number,
                    chunk_hash=chunk_hash,
                    base_url=base_url,
                )

                # Use curl to upload the chunk
                curl_command = f'curl -X PUT -H "Content-Type: application/octet-stream" -T {shlex.quote(temp_chunk_path)} --silent --show-error --fail --max-time 300 --dump-header - {shlex.quote(chunk_upload_url)}'

                # Execute curl command
                result = subprocess.run(
                    shlex.split(curl_command),
                    capture_output=True,
                    text=True,
                    timeout=320,  # Slightly longer than curl's timeout
                )

                if result.returncode != 0:
                    raise Exception(
                        f"Curl failed with code {result.returncode}: {result.stderr}"
                    )

                # Extract ETag from headers
                etag = ""
                for line in result.stdout.split("\n"):
                    if line.lower().startswith("etag:"):
                        etag = line.split(":", 1)[1].strip().strip('"')
                        break

                if not etag:
                    raise Exception("No ETag found in response headers")

                return etag

            except Exception as e:
                if retry == max_retries - 1:
                    print(
                        f"Failed to upload chunk {chunk_number} after {max_retries} retries: {e}"
                    )
                    raise Exception(f"Chunk upload failed: {e}")
                else:
                    print(
                        f"Retry {retry + 1}/{max_retries} for chunk {chunk_number}: {e}"
                    )
                    time.sleep(2**retry)  # Exponential backoff
    finally:
        # Clean up temporary file
        try:
            os.unlink(temp_chunk_path)
        except Exception:
            pass


def init_multipart_upload(
    api_key: str,
    archive_path: str,
    total_size_bytes: int,
    tags: Optional[List[str]] = None,
    base_url: str = API_BASE_URL,
    video_filename: Optional[str] = None,
    control_filename: Optional[str] = None,
    video_duration_seconds: Optional[float] = None,
    video_width: Optional[int] = None,
    video_height: Optional[int] = None,
    video_codec: Optional[str] = None,
    video_fps: Optional[float] = None,
) -> dict:
    """Initialize an upload session."""

    payload = {
        "filename": os.path.basename(archive_path),
        "content_type": "application/x-tar",
        "total_size_bytes": total_size_bytes,
        "uploader_hwid": get_hwid(),
        "upload_timestamp": datetime.now().isoformat(),
    }

    if tags:
        payload["tags"] = tags
    if video_filename:
        payload["video_filename"] = video_filename
    if control_filename:
        payload["control_filename"] = control_filename
    if video_duration_seconds is not None:
        payload["video_duration_seconds"] = video_duration_seconds
    if video_width is not None:
        payload["video_width"] = video_width
    if video_height is not None:
        payload["video_height"] = video_height
    if video_codec:
        payload["video_codec"] = video_codec
    if video_fps is not None:
        payload["video_fps"] = video_fps

    headers = {"Content-Type": "application/json", "X-API-Key": api_key}
    url = f"{base_url}/tracker/upload/game_control/multipart/init"

    with requests.Session() as session:
        response = session.post(url, headers=headers, json=payload, timeout=30)
        response.raise_for_status()
        return response.json()


def get_multipart_chunk_upload_url(
    api_key: str,
    upload_id: str,
    chunk_number: int,
    chunk_hash: str,
    base_url: str = API_BASE_URL,
) -> str:
    """Get a pre-signed URL for uploading a specific chunk."""

    payload = {
        "upload_id": upload_id,
        "chunk_number": chunk_number,
        "chunk_hash": chunk_hash,
    }

    headers = {"Content-Type": "application/json", "X-API-Key": api_key}
    url = f"{base_url}/tracker/upload/game_control/multipart/chunk"

    with requests.Session() as session:
        response = session.post(url, headers=headers, json=payload, timeout=30)
        response.raise_for_status()
        return response.json()["upload_url"]


def complete_multipart_upload(
    api_key: str,
    upload_id: str,
    chunk_etags: List[dict],
    base_url: str = API_BASE_URL,
) -> dict:
    """Complete an upload by combining all chunks."""

    payload = {
        "upload_id": upload_id,
        "chunk_etags": chunk_etags,
    }

    headers = {"Content-Type": "application/json", "X-API-Key": api_key}
    url = f"{base_url}/tracker/upload/game_control/multipart/complete"

    with requests.Session() as session:
        response = session.post(url, headers=headers, json=payload, timeout=60)
        response.raise_for_status()
        return response.json()


def get_multipart_upload_status(
    api_key: str,
    upload_id: str,
    base_url: str = API_BASE_URL,
) -> dict:
    """Get the status of an upload to see which chunks need to be retried."""

    headers = {"X-API-Key": api_key}
    url = f"{base_url}/tracker/upload/game_control/multipart/status/{upload_id}"

    with requests.Session() as session:
        response = session.get(url, headers=headers, timeout=30)
        response.raise_for_status()
        return response.json()


def abort_multipart_upload(
    api_key: str,
    upload_id: str,
    base_url: str = API_BASE_URL,
) -> dict:
    """Abort an upload and clean up any uploaded chunks."""

    headers = {"X-API-Key": api_key}
    url = f"{base_url}/tracker/upload/game_control/multipart/abort/{upload_id}"

    with requests.Session() as session:
        response = session.delete(url, headers=headers, timeout=30)
        response.raise_for_status()
        return response.json()


def get_hwid():
    try:
        with open("/sys/class/dmi/id/product_uuid", "r") as f:
            hardware_id = f.read().strip()
    except:
        try:
            # Fallback for Windows
            output = subprocess.check_output("wmic csproduct get uuid").decode()
            hardware_id = output.split("\n")[1].strip()
        except:
            hardware_id = None
    return hardware_id
