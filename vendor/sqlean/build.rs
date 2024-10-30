use std::env;
use std::path::PathBuf;

const PATH: &str = "./bundled/sqlean/src";

fn build_bindings() {
  let bindings = bindgen::Builder::default()
    .header("bindings.h")
    .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
    .generate()
    .expect("Unable to generate bindings");

  let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
  bindings
    .write_to_file(out_path.join("bindings.rs"))
    .expect("Could not write bindings!");
}

fn build_object() {
  println!("cargo::rerun-if-changed=bundled/sqlean");

  let files = [
    // "sqlite3-define.c",
    "define/eval.c",
    "define/extension.c",
    "define/manage.c",
    "define/module.c",
  ];

  let mut cfg = cc::Build::new();

  // Try to be consistent with:
  //   https://github.com/tursodatabase/libsql/blob/7521bc0d91f34a8a3b8776efe32aa0d20f20bf55/libsql-ffi/build.rs#L111
  //
  // Most importantly, define SQLITE_CORE to avoid dyn sqlite3_api symbol dep.
  cfg
    .flag("-std=c11")
    .flag("-DSQLITE_CORE")
    .flag("-DSQLITE_DEFAULT_FOREIGN_KEYS=1")
    .flag("-DSQLITE_ENABLE_API_ARMOR")
    .flag("-DSQLITE_ENABLE_COLUMN_METADATA")
    .flag("-DSQLITE_ENABLE_DBSTAT_VTAB")
    .flag("-DSQLITE_ENABLE_FTS3")
    .flag("-DSQLITE_ENABLE_FTS3_PARENTHESIS")
    .flag("-DSQLITE_ENABLE_FTS5")
    .flag("-DSQLITE_ENABLE_JSON1")
    .flag("-DSQLITE_ENABLE_LOAD_EXTENSION=1")
    .flag("-DSQLITE_ENABLE_MEMORY_MANAGEMENT")
    .flag("-DSQLITE_ENABLE_RTREE")
    .flag("-DSQLITE_ENABLE_STAT2")
    .flag("-DSQLITE_ENABLE_STAT4")
    .flag("-DSQLITE_SOUNDEX")
    .flag("-DSQLITE_THREADSAFE=1")
    .flag("-DSQLITE_USE_URI")
    .flag("-DHAVE_USLEEP=1")
    // cross compile with MinGW
    .flag("-D_POSIX_THREAD_SAFE_FUNCTIONS")
    // Disable SQLEAN's define-eval feature
    .flag("-DDISABLE_DEFINE_EVAL");

  cfg
    .warnings(false)
    .include(PATH)
    .files(files.iter().map(|f| format!("{PATH}/{f}")))
    .compile("define");

  // Tell cargo to tell rustc to link the library.
  println!("cargo:rustc-link-search={PATH}/define");
  println!("cargo:rustc-link-lib=define");

  // Link sqlite lib from libsql-ffi.
  println!("cargo:rustc-link-lib=libsql");
}

fn main() {
  build_object();
  build_bindings();
}
