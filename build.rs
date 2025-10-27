fn main() {
    // Only link to libFLAC if the flac-export feature is enabled
    #[cfg(all(feature = "flac-export", target_os = "linux"))]
    {
        // Use the correct case for Arch Linux
        println!("cargo:rustc-link-lib=FLAC");
        println!("cargo:rustc-link-search=native=/usr/lib");
    }

    #[cfg(all(feature = "flac-export", target_os = "macos"))]
    {
        println!("cargo:rustc-link-lib=FLAC");
        println!("cargo:rustc-link-search=native=/usr/local/lib");
    }

    #[cfg(all(feature = "flac-export", target_os = "windows"))]
    {
        println!("cargo:rustc-link-lib=flac");
    }
}

// fn main()
// {
//     // flac-sys already handles linking to libflac via #[link(name = "flac")]
//     // We only need to tell Cargo where to search for the library
//
//     #[cfg(all(feature = "flac-export", target_os = "linux"))]
//     {
//         println!("cargo:rustc-link-search=native=/usr/lib");
//         println!("cargo:rustc-link-search=native=/usr/lib/x86_64-linux-gnu");
//
//         // Print helpful error message if libflac is not found
//         // This will be shown during linking phase
//         println!("cargo:warning=Building with FLAC export support. If linking fails, install libflac:");
//         println!("cargo:warning=  Debian/Ubuntu: sudo apt-get install libflac-dev");
//         println!("cargo:warning=  Arch Linux: sudo pacman -S flac");
//         println!("cargo:warning=Or build without FLAC export (library): cargo build --lib --no-default-features");
//         println!("cargo:warning=Or build without FLAC export (with UI): cargo build --no-default-features --features ui");
//     }
//
//     #[cfg(all(feature = "flac-export", target_os = "macos"))]
//     {
//         println!("cargo:rustc-link-search=native=/usr/local/lib");
//         println!("cargo:warning=Building with FLAC export support. If linking fails, install libflac:");
//         println!("cargo:warning=  macOS: brew install flac");
//         println!("cargo:warning=Or build without FLAC export (library): cargo build --lib --no-default-features");
//         println!("cargo:warning=Or build without FLAC export (with UI): cargo build --no-default-features --features ui");
//     }
//
//     #[cfg(all(feature = "flac-export", target_os = "windows"))]
//     {
//         // Windows typically uses different paths, adjust as needed
//         println!("cargo:rustc-link-search=native=C:/Program Files/FLAC/lib");
//         println!("cargo:warning=Building with FLAC export support. Ensure libflac is installed.");
//         println!("cargo:warning=Or build without FLAC export (library): cargo build --lib --no-default-features");
//         println!("cargo:warning=Or build without FLAC export (with UI): cargo build --no-default-features --features ui");
//     }
// }