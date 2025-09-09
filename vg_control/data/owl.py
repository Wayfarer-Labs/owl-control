from dotenv import load_dotenv
import os

load_dotenv()

import tarfile
import shutil
import json
from datetime import datetime

from ..constants import ROOT_DIR, MIN_FOOTAGE, MAX_FOOTAGE, RECORDING_WIDTH, RECORDING_HEIGHT, FPS

from .input_utils.buttons import get_button_stats
from .input_utils.mouse import get_mouse_stats
from .uploader import upload_archive

# Directory structure might be nested, but the root dirs will always have a .mp4 and .csv

def filter_invalid_sample(vid_path, csv_path, meta_path, verbose = False):
    """
    Detect invalid videos
    """
    with open(meta_path) as f:
        metadata = json.load(f)
    duration = float(metadata['duration'])
    
    is_invalid = False

    if duration < MIN_FOOTAGE:
        if verbose:
            print(f"Video length {duration:.2f} too short.")
        is_invalid = True
    if duration > MAX_FOOTAGE + 10:
        if verbose:
            print(f"Video length {duration:.2f} too long.")
    
    bitrate = 2 # mbps
    # Get video file size in MB
    vid_size = os.path.getsize(vid_path) / (1024 * 1024)
    vid_size *= 8 # megabits
    expected_bits = bitrate * duration

    if vid_size < 0.25 * expected_bits:  # Less than quarter of expected size is unlikely
        if verbose:
            print(f"Video size {vid_size:.2f} Mb too small compared to expected {expected_bits:.2f} Mb")
        is_invalid = True
    
    btn_stats = get_button_stats(csv_path)
    mouse_stats = get_mouse_stats(csv_path)

    # Filter out samples with too little activity
    if btn_stats['wasd_apm'] < 10:  # Less than 20 actions per minute is likely AFK/inactive
        if verbose:
            print(f"WASD actions per minute too low: {btn_stats['wasd_apm']:.1f}")
        is_invalid = True
        
    if btn_stats['total_keyboard_events'] < 100:  # Too few keyboard events overall
        if verbose:
            print(f"Too few keyboard events: {btn_stats['total_keyboard_events']}")
        is_invalid = True

    # Filter out samples with abnormal mouse behavior
    if mouse_stats['overall_max'] < 0.05:  # Very little mouse movement
        if verbose:
            print(f"Mouse movement too small: {mouse_stats['overall_max']:.3f}")
        is_invalid = True
        
    if mouse_stats['overall_max'] > 500:  # Unreasonably large mouse movements
        if verbose:
            print(f"Mouse movement too large: {mouse_stats['overall_max']:.1f}")
        is_invalid = True

    # Add stats to metadata
    extra_metadata = {
        'input_stats': {
            'wasd_apm': btn_stats['wasd_apm'],
            'unique_keys': btn_stats['unique_keys'], 
            'button_diversity': btn_stats['button_diversity'],
            'total_keyboard_events': btn_stats['total_keyboard_events'],
            'mouse_movement_std': mouse_stats['overall_std'],
            'mouse_x_std': mouse_stats['x_std'],
            'mouse_y_std': mouse_stats['y_std'],
            'mouse_max_movement': mouse_stats['overall_max'],
            'mouse_max_x': mouse_stats['max_x'],
            'mouse_max_y': mouse_stats['max_y']
        }
    }

    if not 'input_stats' in metadata:
        metadata.update(extra_metadata)
        with open(meta_path, 'w') as f:
            json.dump(metadata, f, indent=4)

    return is_invalid

class OWLDataManager:
    def __init__(self, token, progress_mode=False, bundle_sessions=False):
        self.staged_files = []
        self.staging_dir = "staging"
        self.current_tar_uuid = None
        self.token = token
        self.progress_mode = progress_mode
        self.bundle_sessions = bundle_sessions  # If False, create individual tars per session
        self.total_duration = 0.0  # Track total duration of uploaded videos
        self.total_bytes = 0  # Track total bytes of files
        self.staged_bytes = 0  # Track bytes staged so far
        os.makedirs(self.staging_dir, exist_ok=True)

    def _process_individual_sessions(self, verbose=False):
        """Process each session as an individual tar file and upload immediately."""
        sessions_processed = 0
        
        for root, dirs, files in os.walk(ROOT_DIR):
            if '.uploaded' in files or '.invalid' in files:
                continue
            
            has_mp4 = any([fname.endswith('.mp4') for fname in files])
            has_csv = any([fname.endswith('.csv') for fname in files])
            has_metadata = any([fname == 'metadata.json' for fname in files])
            
            if has_mp4 and has_csv and has_metadata:
                mp4_file = next(f for f in files if f.endswith('.mp4'))
                csv_file = next(f for f in files if f.endswith('.csv'))
                
                mp4_path = os.path.join(root, mp4_file)
                csv_path = os.path.join(root, csv_file)
                meta_path = os.path.join(root, 'metadata.json')
                
                # Check validity
                try:
                    invalid = filter_invalid_sample(mp4_path, csv_path, meta_path, verbose)
                except Exception as e:
                    print(f"Warning: Invalid data skipped: {e}")
                    invalid = True
                
                if invalid:
                    with open(os.path.join(root, '.invalid'), 'w') as f:
                        pass
                    continue
                
                # Read duration from metadata and track bytes
                metadata_dict = {}
                try:
                    with open(meta_path) as f:
                        metadata_dict = json.load(f)
                    duration = float(metadata_dict.get('duration', 0))
                    self.total_duration += duration
                except Exception as e:
                    print(f"Warning: Could not read duration from {meta_path}: {e}")
                
                # Track file sizes for statistics
                mp4_size = os.path.getsize(mp4_path)
                csv_size = os.path.getsize(csv_path)
                meta_size = os.path.getsize(meta_path)
                self.total_bytes += mp4_size + csv_size + meta_size
                
                # Create tar for this single session
                import uuid
                tar_name = f"{uuid.uuid4().hex[:16]}.tar"
                
                with tarfile.open(tar_name, "w") as tar:
                    tar.add(mp4_path, arcname=mp4_file)
                    tar.add(csv_path, arcname=csv_file)
                    tar.add(meta_path, arcname='metadata.json')
                
                # Upload immediately with metadata
                try:
                    upload_archive(
                        self.token, 
                        tar_name, 
                        progress_mode=self.progress_mode,
                        video_filename=mp4_file,
                        control_filename=csv_file,
                        video_duration_seconds=metadata_dict.get('duration') if metadata_dict else None,
                        video_width=RECORDING_WIDTH,
                        video_height=RECORDING_HEIGHT,
                        video_fps=FPS
                        # video_codec not set here since it depends on user's OBS settings
                    )
                    with open(os.path.join(root, '.uploaded'), 'w') as f:
                        f.write('')
                    self.staged_files.append(root)
                    sessions_processed += 1
                finally:
                    if os.path.exists(tar_name):
                        os.remove(tar_name)
        
        return sessions_processed > 0

    def stage(self, verbose = False):
        # If not bundling, process each session individually
        if not self.bundle_sessions:
            return self._process_individual_sessions(verbose)
        
        file_counter = 0
        total_folders = sum(1 for root, dirs, files in os.walk(ROOT_DIR) if not ('.uploaded' in files or '.invalid' in files))
        processed_folders = 0
        
        # First pass: calculate total bytes of all valid files
        if self.progress_mode:
            progress_data = {"phase": "calculating", "action": "start", "message": "Calculating total size..."}
            print(f"PROGRESS: {json.dumps(progress_data)}")
        
        for root, dirs, files in os.walk(ROOT_DIR):
            if '.uploaded' in files or '.invalid' in files:
                continue
                
            has_mp4 = any([fname.endswith('.mp4') for fname in files])
            has_csv = any([fname.endswith('.csv') for fname in files])
            has_metadata = any([fname == 'metadata.json' for fname in files])
                
            if has_mp4 and has_csv and has_metadata:
                mp4_file = next(f for f in files if f.endswith('.mp4'))
                csv_file = next(f for f in files if f.endswith('.csv'))
                metadata_file = 'metadata.json'
                
                mp4_src_path = os.path.join(root, mp4_file)
                csv_src_path = os.path.join(root, csv_file)
                meta_src_path = os.path.join(root, metadata_file)
                
                # Check if file would be invalid before counting bytes
                try:
                    invalid = filter_invalid_sample(mp4_src_path, csv_src_path, meta_src_path, verbose = False)
                    if not invalid:
                        self.total_bytes += os.path.getsize(mp4_src_path)
                        self.total_bytes += os.path.getsize(csv_src_path)
                        self.total_bytes += os.path.getsize(meta_src_path)
                except Exception:
                    pass  # Skip invalid files
        
        if self.progress_mode:
            progress_data = {"phase": "staging", "action": "start", "total_folders": total_folders, "total_bytes": self.total_bytes}
            print(f"PROGRESS: {json.dumps(progress_data)}")
        
        for root, dirs, files in os.walk(ROOT_DIR):
            if '.uploaded' in files or '.invalid' in files:
                continue

            processed_folders += 1
            
            if verbose: 
                print(root, dirs, files)

            has_mp4 = any([fname.endswith('.mp4') for fname in files])
            has_csv = any([fname.endswith('.csv') for fname in files])
            has_metadata = any([fname == 'metadata.json' for fname in files])
                
            if has_mp4 and has_csv and has_metadata:
                mp4_file = next(f for f in files if f.endswith('.mp4'))
                csv_file = next(f for f in files if f.endswith('.csv'))
                metadata_file = 'metadata.json'
                
                new_mp4 = f"{file_counter:06d}.mp4"
                new_csv = f"{file_counter:06d}.csv"
                new_metadata = f"{file_counter:06d}.json"
                
                os.makedirs(self.staging_dir, exist_ok=True)
                
                # Copy files to staging with new names
                mp4_src_path = os.path.join(root, mp4_file)
                csv_src_path = os.path.join(root, csv_file)
                meta_src_path = os.path.join(root, metadata_file)

                if self.progress_mode:
                    progress_data = {"phase": "staging", "action": "processing", "current_file": mp4_file, "processed": file_counter}
                    print(f"PROGRESS: {json.dumps(progress_data)}")

                try:
                    invalid = filter_invalid_sample(mp4_src_path, csv_src_path, meta_src_path, verbose = True)
                except Exception as e:
                    print(f"Warning: Invalid data skipped by uploader: {e}")
                    invalid = True

                if invalid:
                    with open(os.path.join(root, '.invalid'), 'w') as f:
                        pass
                    if self.progress_mode:
                        progress_data = {"phase": "staging", "action": "invalid", "current_file": mp4_file}
                        print(f"PROGRESS: {json.dumps(progress_data)}")
                    continue

                # Read duration from metadata and add to total
                try:
                    with open(meta_src_path) as f:
                        metadata = json.load(f)
                    duration = float(metadata.get('duration', 0))
                    self.total_duration += duration
                except Exception as e:
                    print(f"Warning: Could not read duration from {meta_src_path}: {e}")

                # Get file sizes for progress tracking
                mp4_size = os.path.getsize(mp4_src_path)
                csv_size = os.path.getsize(csv_src_path)
                meta_size = os.path.getsize(meta_src_path)
                file_total_bytes = mp4_size + csv_size + meta_size

                shutil.copy2(mp4_src_path, os.path.join(self.staging_dir, new_mp4))
                shutil.copy2(csv_src_path, os.path.join(self.staging_dir, new_csv))
                shutil.copy2(meta_src_path, os.path.join(self.staging_dir, new_metadata))

                self.staged_files.append(root)
                file_counter += 1
                self.staged_bytes += file_total_bytes
                
                if self.progress_mode:
                    progress_data = {
                        "phase": "staging", 
                        "action": "staged", 
                        "current_file": mp4_file,
                        "files_staged": file_counter,
                        "total_files": None,  # We don't know total until we finish
                        "bytes_staged": self.staged_bytes,
                        "total_bytes": self.total_bytes
                    }
                    print(f"PROGRESS: {json.dumps(progress_data)}")
        
        if self.progress_mode:
            progress_data = {"phase": "staging", "action": "complete", "total_files": file_counter}
            print(f"PROGRESS: {json.dumps(progress_data)}")
        
        return file_counter > 0
    

    def compress(self):
        # Skip if not bundling (already handled in stage)
        if not self.bundle_sessions:
            return None
            
        import uuid
        self.current_tar_uuid = uuid.uuid4().hex[:16]
        tar_name = f"{self.current_tar_uuid}.tar"
        
        if self.progress_mode:
            progress_data = {"phase": "compress", "action": "start", "tar_file": tar_name}
            print(f"PROGRESS: {json.dumps(progress_data)}")
        
        with tarfile.open(tar_name, "w") as tar:
            files_to_add = os.listdir(self.staging_dir)
            total_files = len(files_to_add)
            for i, file in enumerate(files_to_add):
                tar.add(os.path.join(self.staging_dir, file), arcname=file)
                if self.progress_mode:
                    progress_data = {
                        "phase": "compress", 
                        "action": "file", 
                        "current_file": file,
                        "files_compressed": i + 1,
                        "total_files": total_files
                    }
                    print(f"PROGRESS: {json.dumps(progress_data)}")
        
        # Update total_bytes to reflect the actual tar file size
        if os.path.exists(tar_name):
            self.total_bytes = os.path.getsize(tar_name)
        
        if self.progress_mode:
            progress_data = {
                "phase": "compress", 
                "action": "complete", 
                "tar_file": tar_name,
                "tar_size_bytes": self.total_bytes
            }
            print(f"PROGRESS: {json.dumps(progress_data)}")
            
        return tar_name

    def upload(self):
        # Skip if not bundling (already handled in stage)
        if not self.bundle_sessions:
            return
            
        if not self.current_tar_uuid:
            raise Exception("Must compress before uploading")
            
        tar_name = f"{self.current_tar_uuid}.tar"
        
        if self.progress_mode:
            progress_data = {"phase": "upload", "action": "start", "tar_file": tar_name}
            print(f"PROGRESS: {json.dumps(progress_data)}")
        
        try:
            # Pass aggregated metadata for bundled uploads
            upload_archive(
                self.token, 
                tar_name, 
                progress_mode=self.progress_mode,
                video_duration_seconds=self.total_duration,  # Total duration of all videos in bundle
                video_width=RECORDING_WIDTH,
                video_height=RECORDING_HEIGHT,
                video_fps=FPS
            )
            
            if self.progress_mode:
                progress_data = {"phase": "upload", "action": "complete", "tar_file": tar_name}
                print(f"PROGRESS: {json.dumps(progress_data)}")
            
            total_staged = len(self.staged_files)
            for i, staged_path in enumerate(self.staged_files):
                with open(os.path.join(staged_path, '.uploaded'), 'w') as f:
                    f.write('')
                if self.progress_mode:
                    progress_data = {
                        "phase": "finalize", 
                        "action": "mark_uploaded", 
                        "current": i + 1,
                        "total": total_staged
                    }
                    print(f"PROGRESS: {json.dumps(progress_data)}")

        finally:
            # Cleanup
            if os.path.exists(tar_name):
                os.remove(tar_name)
            if os.path.exists(self.staging_dir):
                shutil.rmtree(self.staging_dir)
            self.staged_files = []
            self.current_tar_uuid = None

    def clear_upload_status(self):
        for root, dirs, files in os.walk(ROOT_DIR):
            if '.uploaded' in files:
                os.remove(os.path.join(root, '.uploaded'))

    def delete_uploaded(self):
        for root, dirs, files in os.walk(ROOT_DIR):
            if '.uploaded' in files:
                shutil.rmtree(root)


def upload_all_files(token, delete_uploaded=False, progress_mode=False, bundle_sessions=False):
    manager = OWLDataManager(token, progress_mode=progress_mode, bundle_sessions=bundle_sessions)
    has_files = manager.stage()
    if has_files:
        manager.compress()
        manager.upload()

    if delete_uploaded:
        manager.delete_uploaded()
    
    # Output final stats for the main process to capture
    if progress_mode:
        final_stats = {
            "phase": "complete",
            "total_files_uploaded": len(manager.staged_files),
            "total_duration_uploaded": manager.total_duration,
            "total_bytes_uploaded": manager.total_bytes
        }
        print(f"FINAL_STATS: {json.dumps(final_stats)}")
    
    return {
        "files_uploaded": len(manager.staged_files) if has_files else 0,
        "total_duration": manager.total_duration
    }

if __name__ == "__main__":
    import sys
    
    if len(sys.argv) < 2:
        print("Error: Token argument is required")
        sys.exit(1)
        
    token = sys.argv[1]
    del_uploaded = False
    if len(sys.argv) > 2:
        del_uploaded = sys.argv[2].lower() == 'true'

    try:
        upload_all_files(token, delete_uploaded=del_uploaded)
        print("Upload completed successfully")
    except Exception as e:
        print(f"Error during upload: {str(e)}")
        sys.exit(1)