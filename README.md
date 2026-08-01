# BitPlay

A Rust application for analyzing and processing MP4 video files. BitPlay reads MP4 metadata and retrieves detailed video information using ffprobe.

## Prerequisites

- Rust (1.70+)
- Cargo
- ffprobe (for video metadata extraction)

### Installing ffprobe

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get install ffmpeg
```

**macOS:**
```bash
brew install ffmpeg
```

**Windows:**
Download from [ffmpeg.org](https://ffmpeg.org/download.html) or use a package manager like Chocolatey.

## Building

Build the project in release mode:

```bash
cargo build 
```

The compiled binary will be located at `target/release/BitPlay`.

## Running

### Basic Usage

Run the application with an MP4 file:

```bash
cargo run -- <path-to-video.mp4>
```

Example:
```bash
cargo run -- test.mp4
```

### Direct Execution

After building, you can run the binary directly:

```bash
./target/release/BitPlay test.mp4
```

## Testing

To test with a sample MP4 file:

```bash
cargo run -- test.mp4
```

This will:
1. Parse the MP4 file structure
2. Extract metadata using ffprobe
3. Display video information (duration, FPS, codec profile, encoder, etc.)

## Project Structure

- `src/main.rs` - Main application entry point
- `Cargo.toml` - Project dependencies and configuration

## Dependencies

- **mp4** (0.14) - MP4 file format parsing
- **anyhow** (1.0) - Error handling
- **serde** (1.0) - Serialization framework
- **serde_json** (1.0) - JSON processing

## License

MIT
