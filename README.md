# Purpose

Use toml files to do no-code generating bindings to C libraries.

1. It can be used as a replacement of `-sys` crates, each of which should
have generated bindings separately for one single C library.

2. It can help a standalone `-sys` crate to generate bindings. The users do not
need to learn the usage of `pkg-config` and `bindgen` crates.

# Requirements

1. C libraries must provide pkg-config file.

2. C libraries can be compiled with `bindgen`'s default cofiguration.

# Bundled C libraries

The toml files of the following libraries are bundled in the `clib_spec/`
directory: libcurl, liblzma, sqlite3, tcl86, tk86, x11, zlib. This list
can grow in the future.

These are also crate features.

## Example of generating "sqlite3" bindings

Add this crate to Cargo.toml, and enable the "sqlite3" feature.

```toml
clib = { version = "0.1", features = "sqlite3" }
```

All generated functions, types and constants are in the root namespace of this
crate. You can prefix them with `clib::`, e.g. `clib::sqlite3_open()`, or
`use clib::*;` and use `sqlite3_open()` directly.

# Extra C libraries

Use the environment variable `CLIB_EXTRA_LIBS` to assign a whitespace-separated
list of C library names which are not provided as crate features. Missing toml
files are provided via searching `CLIB_SPEC_DIRS`. See section below.

*Note: the library name must be accordant to the ".pc" file name.*

# Extra directories of toml files

Use the environment variable `CLIB_SPEC_DIRS` to assign a semicolon-separated
list of extra search paths for toml files.

This list can

1. provide the locations of toml files which are not bundled by this crate.

2. override bundled toml files. For example, to override `sqlite3.toml`, just
put your modified file in downstream crate's `clib_spec/` directory.


*Note: Absolute paths are preferred over relative ones, because the latters are
NOT relative to downstream crates but to THIS CRATE.*

# Set minimum version requirement

Use the environment variable `CLIB_{}_MIN_VER` to set a min version requirement
for the library `{}`. The absence of the variable means "any version is ok".

# The toml configuration file's syntax

## The "header" section

Currently this is the only supported section in toml configuration file.

1. `files`: a list of C headers to generate bindings for.

2. `import`: a list of dependencies of other C libraries' names. It is optional.

3. `import_dir`: a list of C libraries' names, hearders of which are included in
current library's header(s). It is optional.

Take `tk86.toml` for example:

```toml
[header]
files = [ "tk.h" ]
import = [ "tcl86" ]
import_dir = [ "x11" ]
```

The value of `files` indicates that tk.h is the public header for tk86.

The value of `import` indicates that tcl86 is an upstream library which need
also generate bindings for.

The value of `x11` indicates that x11's include directory must be added into
the current library(tk86)'s header search path. This is required because tk86's
headers include x11's headers.

# License

Under Apache License 2.0 or MIT License, at your will.
