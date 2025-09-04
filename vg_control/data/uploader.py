import os
from typing import List, Optional
from datetime import datetime
import requests
import urllib3
import subprocess
import shlex
from tqdm import tqdm
import time

from ..constants import API_BASE_URL
from ..metadata import get_hwid

def _get_upload_url(
    api_key: str,
    archive_path: str,
    tags: Optional[List[str]] = None,
    base_url: str = API_BASE_URL,
) -> str:
    """Request a pre-signed S3 URL for uploading a tar archive."""

    file_size_mb = os.path.getsize(archive_path) // (1024 * 1024)
    payload = {
        "filename": os.path.basename(archive_path),
        "content_type": "application/x-tar", 
        "file_size_mb": file_size_mb,
        "expiration": 3600,
        "uploader_hwid": get_hwid(),
        "upload_timestamp": datetime.now().isoformat()
    }
    if tags:
        payload["tags"] = tags

    headers = {"Content-Type": "application/json", "X-API-Key": api_key}
    url = f"{base_url}/tracker/upload/game_control"

    response = requests.post(url, headers=headers, json=payload, timeout=30)
    response.raise_for_status()
    data = response.json()
    return data.get("url") or data.get("upload_url") or data["uploadUrl"]

def _upload_archive(
    api_key: str,
    archive_path: str,
    tags: Optional[List[str]] = None,
    base_url: str = API_BASE_URL,
) -> None:
    """Upload an archive to the storage bucket via a pre-signed URL."""

    upload_url = get_upload_url(
        api_key,
        archive_path,
        tags=tags,
        base_url=base_url,
    )

    with open(archive_path, "rb") as f:
        put_resp = requests.put(upload_url, data=f, timeout=60, verify=False)
        put_resp.raise_for_status()

def get_upload_url(
    api_key: str,
    archive_path: str,
    tags: Optional[List[str]] = None,
    base_url: str = API_BASE_URL,
) -> str:
    """Request a pre-signed S3 URL for uploading a tar archive."""

    file_size = os.path.getsize(archive_path)
    file_size_mb = file_size // (1024 * 1024)
    payload = {
        "filename": os.path.basename(archive_path),
        "content_type": "application/x-tar", 
        "file_size_mb": file_size_mb,
        "expiration": 3600,
        "uploader_hwid": get_hwid(),
        "upload_timestamp": datetime.now().isoformat()
    }
    if tags:
        payload["tags"] = tags

    headers = {"Content-Type": "application/json", "X-API-Key": api_key}
    url = f"{base_url}/tracker/upload/game_control"

    with requests.Session() as session:
        response = session.post(url, headers=headers, json=payload, timeout=30)
        response.raise_for_status()
        data = response.json()
        return data.get("url") or data.get("upload_url") or data["uploadUrl"]

def upload_archive(
    api_key: str,
    archive_path: str,
    tags: Optional[List[str]] = None,
    base_url: str = API_BASE_URL,
    progress_mode: bool = False,
) -> None:
    """Upload an archive to the storage bucket via a pre-signed URL."""

    upload_url = get_upload_url(
        api_key,
        archive_path,
        tags=tags,
        base_url=base_url,
    )

    # Get file size for progress bar
    import os
    file_size = os.path.getsize(archive_path)
    
    # Initialize progress file
    if progress_mode:
        import tempfile
        import json
        progress_file = os.path.join(tempfile.gettempdir(), 'owl-control-upload-progress.json')
        initial_progress = {
            "phase": "upload",
            "action": "start",
            "bytes_uploaded": 0,
            "total_bytes": file_size,
            "percent": 0,
            "speed_mbps": 0,
            "eta_seconds": 0,
            "timestamp": time.time()
        }
        try:
            with open(progress_file, 'w') as f:
                json.dump(initial_progress, f)
        except Exception as e:
            print(f"Warning: Could not initialize progress file: {e}")
    
    def emit_upload_progress(bytes_uploaded, total_bytes, speed_bps=0):
        """Write JSON progress data to file for UI consumption"""
        if progress_mode:
            import json
            import tempfile
            import os
            
            progress_data = {
                "phase": "upload",
                "action": "progress",
                "bytes_uploaded": bytes_uploaded,
                "total_bytes": total_bytes,
                "percent": min((bytes_uploaded / total_bytes) * 100, 100) if total_bytes > 0 else 0,
                "speed_mbps": speed_bps / (1024 * 1024) if speed_bps > 0 else 0,
                "eta_seconds": ((total_bytes - bytes_uploaded) / speed_bps) if speed_bps > 0 else 0,
                "timestamp": time.time()
            }
            
            # Write to temp file for UI to read
            progress_file = os.path.join(tempfile.gettempdir(), 'owl-control-upload-progress.json')
            try:
                with open(progress_file, 'w') as f:
                    json.dump(progress_data, f)
            except Exception as e:
                print(f"Warning: Could not write progress file: {e}")
            
            # Also print for console (keep existing behavior)
            print(f"PROGRESS: {json.dumps(progress_data)}")
    
    # Use requests with streaming for reliable large file uploads
    # No timeout is set to allow uploads of any size to complete
    
    # Debug: log the upload URL (hide sensitive parts)
    from urllib.parse import urlparse
    parsed_url = urlparse(upload_url)
    
    # Write to debug log file
    import tempfile
    debug_log_path = os.path.join(tempfile.gettempdir(), 'owl-control-debug.log')
    try:
        with open(debug_log_path, 'a') as debug_file:
            debug_file.write(f"[{datetime.now().isoformat()}] PYTHON: Uploading to host: {parsed_url.netloc}\n")
            debug_file.write(f"[{datetime.now().isoformat()}] PYTHON: Full URL length: {len(upload_url)} chars\n")
    except:
        pass  # Don't fail if debug logging fails
    
    # Upload with requests using streaming to handle large files reliably
    start_time = time.time()
    bytes_uploaded = 0
    chunk_size = 1024 * 1024  # 1MB chunks for smooth progress
    
    try:
        with open(archive_path, 'rb') as f:
            with tqdm(total=file_size, unit='B', unit_scale=True, desc="Uploading") as pbar:
                
                def file_reader():
                    """Generator that reads file in chunks and updates progress"""
                    nonlocal bytes_uploaded
                    last_progress_time = time.time()
                    
                    while True:
                        chunk = f.read(chunk_size)
                        if not chunk:
                            break
                        
                        chunk_len = len(chunk)
                        bytes_uploaded += chunk_len
                        pbar.update(chunk_len)
                        
                        # Update progress file periodically (every 0.5 seconds)
                        current_time = time.time()
                        if progress_mode and (current_time - last_progress_time > 0.5):
                            elapsed = current_time - start_time
                            speed_bps = bytes_uploaded / elapsed if elapsed > 0 else 0
                            emit_upload_progress(bytes_uploaded, file_size, speed_bps)
                            last_progress_time = current_time
                        
                        yield chunk
                    
                    # Final progress update
                    if progress_mode:
                        elapsed = time.time() - start_time
                        speed_bps = bytes_uploaded / elapsed if elapsed > 0 else 0
                        emit_upload_progress(bytes_uploaded, file_size, speed_bps)
                
                # Perform the upload with no timeout
                response = requests.put(
                    upload_url,
                    data=file_reader(),
                    headers={
                        'Content-Type': 'application/x-tar',
                        'Content-Length': str(file_size)
                    },
                    verify=False  # Skip SSL verification (equivalent to curl -k)
                )
                
                # Check if upload was successful
                response.raise_for_status()
                return_code = 0
                
    except requests.exceptions.RequestException as e:
        print(f"Upload failed: {e}")
        return_code = 1
        # Log error to debug file
        try:
            with open(debug_log_path, 'a') as debug_file:
                debug_file.write(f"[{datetime.now().isoformat()}] ERROR: {str(e)}\n")
        except:
            pass
    except Exception as e:
        print(f"Unexpected error: {e}")
        return_code = 1
        try:
            with open(debug_log_path, 'a') as debug_file:
                debug_file.write(f"[{datetime.now().isoformat()}] ERROR: {str(e)}\n")
        except:
            pass
    
    # Cleanup progress file
    if progress_mode:
        import tempfile
        import os
        progress_file = os.path.join(tempfile.gettempdir(), 'owl-control-upload-progress.json')
        try:
            if os.path.exists(progress_file):
                # Write final completion state
                final_progress = {
                    "phase": "upload",
                    "action": "complete",
                    "bytes_uploaded": file_size,
                    "total_bytes": file_size,
                    "percent": 100,
                    "speed_mbps": 0,
                    "eta_seconds": 0,
                    "timestamp": time.time()
                }
                with open(progress_file, 'w') as f:
                    json.dump(final_progress, f)
        except Exception as e:
            print(f"Warning: Could not write final progress: {e}")
    
    if return_code != 0:
        raise Exception(f"Upload failed with return code {return_code}")
