import os
import sys
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
    unreliable_connections: bool = False,
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
        chunk_size_bytes=5 * 1024 * 1024 if unreliable_connections else None,
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
                            start_time=start_time,
                            total_bytes=file_size,
                            total_chunks=total_chunks,
                            bytes_uploaded_before_chunk=bytes_uploaded,
                            unreliable_connections=unreliable_connections,
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

                        # Final progress update after chunk completion
                        emit_upload_progress(
                            bytes_uploaded=bytes_uploaded,
                            start_time=start_time,
                            total_bytes=file_size,
                            current_chunk=chunk_num,
                            total_chunks=total_chunks,
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
    start_time: float = 0,
    total_bytes: int = 0,
    total_chunks: int = 0,
    bytes_uploaded_before_chunk: int = 0,
    unreliable_connections: bool = False,
) -> str:
    """
    Upload a single chunk using curl with retry logic and progress tracking.

    Args:
        chunk_data: The chunk data to upload
        chunk_number: The chunk number (for logging)
        max_retries: Maximum number of retry attempts
         start_time: Start time for speed calculations
        total_bytes: Total file size for progress calculations
        total_chunks: Total number of chunks
        bytes_uploaded_before_chunk: Bytes uploaded before this chunk

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
            total_bytes_uploaded = bytes_uploaded_before_chunk
            try:
                # Get upload URL for this chunk
                chunk_upload_url = get_multipart_chunk_upload_url(
                    api_key=api_key,
                    upload_id=upload_id,
                    chunk_number=chunk_number,
                    chunk_hash=chunk_hash,
                    base_url=base_url,
                )

                timeout = 60 * 60 if unreliable_connections else 5 * 60

                # Use curl to upload the chunk with progress output
                curl_command = f'curl -X PUT -H "Content-Type: application/octet-stream" -T {shlex.quote(temp_chunk_path)} -# --show-error --fail --max-time {timeout} --dump-header - {shlex.quote(chunk_upload_url)}'

                # Execute curl command with real-time progress parsing
                process = subprocess.Popen(
                    shlex.split(curl_command),
                    stderr=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    bufsize=1,  # Line buffered
                    universal_newlines=True,
                )

                # Parse curl progress output and headers
                stdout_lines = []
                stderr_lines = []
                last_progress_update = 0
                chunk_size = len(chunk_data)

                while True:
                    # Read from stderr (progress output) and stdout (headers)
                    if process.stderr.readable():
                        try:
                            stderr_line = process.stderr.readline()
                            if stderr_line:
                                stderr_lines.append(stderr_line)
                                # Parse progress from curl's -# output
                                if "#" in stderr_line:
                                    try:
                                        # Extract percentage from the number of # characters
                                        percent = min(
                                            (stderr_line.count("#") / 50) * 100, 100
                                        )
                                        current_chunk_bytes = min(
                                            int(chunk_size * (percent / 100)),
                                            chunk_size,
                                        )

                                        # Only update if we've made progress to avoid unnecessary refreshes
                                        if current_chunk_bytes > last_progress_update:
                                            # Calculate total bytes uploaded including this chunk's progress
                                            total_bytes_uploaded = (
                                                bytes_uploaded_before_chunk
                                                + current_chunk_bytes
                                            )

                                            # Emit progress update
                                            emit_upload_progress(
                                                bytes_uploaded=total_bytes_uploaded,
                                                start_time=start_time,
                                                total_bytes=total_bytes,
                                                current_chunk=chunk_number,
                                                total_chunks=total_chunks,
                                            )

                                            last_progress_update = current_chunk_bytes
                                    except Exception as e:
                                        # Don't fail the upload if progress parsing fails
                                        print(f"Warning: Progress parsing error: {e}")
                                        continue
                        except Exception:
                            pass

                    # Check if process is still running
                    if process.poll() is not None:
                        break

                try:
                    if process.stdout.readable():
                        while True:
                            stdout_line = process.stdout.readline()
                            if not stdout_line:
                                break
                            stdout_lines.append(stdout_line)
                except Exception:
                    pass

                # Wait for process to complete
                return_code = process.wait()

                if return_code != 0:
                    stderr_content = "".join(stderr_lines)
                    raise Exception(
                        f"Curl failed with code {return_code}: {stderr_content}"
                    )

                # Extract ETag from headers
                etag = ""
                stdout_content = "".join(stdout_lines)
                for line in stdout_content.split("\n"):
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
    chunk_size_bytes: Optional[int] = None,
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
    if chunk_size_bytes is not None:
        payload["chunk_size_bytes"] = chunk_size_bytes

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


def emit_upload_progress(
    bytes_uploaded: int,
    start_time: float,
    total_bytes: int,
    current_chunk: int,
    total_chunks: int,
):
    """Write JSON progress data to file for UI consumption"""

    elapsed_time = time.time() - start_time
    speed_bps = bytes_uploaded / elapsed_time if elapsed_time > 0 else 0

    progress_data = {
        "bytes_uploaded": bytes_uploaded,
        "total_bytes": total_bytes,
        "percent": min((bytes_uploaded / total_bytes) * 100, 100)
        if total_bytes > 0
        else 0,
        "speed_mbps": speed_bps / (1024 * 1024) if speed_bps > 0 else 0,
        "eta_seconds": ((total_bytes - bytes_uploaded) / speed_bps)
        if speed_bps > 0
        else 0,
        "current_chunk": current_chunk,
        "total_chunks": total_chunks,
    }

    print(f"PROGRESS: {json.dumps(progress_data)}", file=sys.stderr)
    sys.stderr.flush()


def get_hwid():
    try:
        with open("/sys/class/dmi/id/product_uuid", "r") as f:
            hardware_id = f.read().strip()
    except Exception:
        try:
            # Fallback for Windows
            output = subprocess.check_output("wmic csproduct get uuid").decode()
            hardware_id = output.split("\n")[1].strip()
        except Exception:
            hardware_id = None
    return hardware_id
