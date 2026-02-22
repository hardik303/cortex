fn main() {
    // Compile Objective-C OCR wrapper
    cc::Build::new()
        .file("src-objc/ocr_wrapper.m")
        .flag("-fmodules")
        .flag("-fobjc-arc")
        .flag("-Wno-deprecated-declarations")
        .compile("ocr_wrapper");

    // Compile Objective-C window info wrapper
    cc::Build::new()
        .file("src-objc/window_info.m")
        .flag("-fmodules")
        .flag("-fobjc-arc")
        .flag("-Wno-deprecated-declarations")
        .compile("window_info");

    // Compile Objective-C app metadata wrapper (browser + terminal)
    cc::Build::new()
        .file("src-objc/app_metadata.m")
        .flag("-fmodules")
        .flag("-fobjc-arc")
        .flag("-Wno-deprecated-declarations")
        .compile("app_metadata");

    // Link Apple frameworks
    println!("cargo:rustc-link-lib=framework=Vision");
    println!("cargo:rustc-link-lib=framework=CoreImage");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");

    // Re-run if ObjC sources change
    println!("cargo:rerun-if-changed=src-objc/ocr_wrapper.h");
    println!("cargo:rerun-if-changed=src-objc/ocr_wrapper.m");
    println!("cargo:rerun-if-changed=src-objc/window_info.h");
    println!("cargo:rerun-if-changed=src-objc/window_info.m");
    println!("cargo:rerun-if-changed=src-objc/app_metadata.h");
    println!("cargo:rerun-if-changed=src-objc/app_metadata.m");
}
