use bindgen::Builder;
use pkg_config::{Config, Library};
use serde_derive::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    env,
    fmt::Debug,
    fs::File,
    io::{self, Read},
    ops::RangeBounds,
    path::{Path, PathBuf},
    process::Command,
    sync::Once,
};

static mut EXTRA_LIBS  : &'static str = "";
static INIT_EXTRA_LIBS : Once = Once::new();

fn extra_libs() -> &'static str {
    // write once and read all is safe
    unsafe {
        INIT_EXTRA_LIBS.call_once( || {
            env::var("CLIB_EXTRA_LIBS")
                .map( |var| EXTRA_LIBS = Box::leak( var.into_boxed_str() ))
                .ok();
        });
        &EXTRA_LIBS
    }
}

static mut SPEC_DIRS  : &'static str = ".";
static INIT_SPEC_DIRS : Once = Once::new();

fn specs_dirs() -> &'static str {
    // write once and read all is safe
    unsafe {
        INIT_SPEC_DIRS.call_once( || {
            env::var("CLIB_SPEC_DIRS")
                .map( |var| SPEC_DIRS = Box::leak( var.into_boxed_str() ))
                .ok();
        });
        &SPEC_DIRS
    }
}

#[derive( Debug, Deserialize )]
struct Meta {
    alias: HashMap<String,String>,
}

#[derive( Debug, Deserialize )]
struct Spec {
    header: Header,
}

#[derive( Debug, Deserialize )]
struct Header {
    files      : Vec<String>,
    import     : Option<Vec<String>>,
    import_dir : Option<Vec<String>>,
}

fn probe<'n,'v>( name: &'n str, version: impl RangeBounds<&'v str> ) -> Result<Library, pkg_config::Error> {
    let mut cfg = Config::new();
    cfg.cargo_metadata( true ).range_version( version );
    Ok( cfg.probe( name )? )
}

fn load_toml<P: AsRef<Path> +Clone +Debug>( name: P ) -> Result<String, io::Error> {
    for dir in specs_dirs().split(';').chain( Some(".")) {
        let path = Path::new( &dir ).join( name.clone() );
        let mut meta = match File::open( path ) {
            Ok( meta ) => meta,
            Err( _ ) => continue,
        };
        let mut contents = String::new();
        meta.read_to_string( &mut contents )?;
        return Ok( contents );
    }
    panic!( "{:?} not found", name );
}

const UTF8_PATH: &'static str = "path should be valid UTF-8 string.";

fn load_spec( name: &str ) -> Option<String> {
    let path = Path::new( "clib_spec" ).join( name ).with_extension( "toml" );
    load_toml( path ).ok()
}

#[derive( Debug, Default )]
pub struct LibInfo {
    link_paths    : Vec<String>,
    include_paths : Vec<String>,
    headers       : Vec<String>,

}

pub fn probe_library<'n,'v>( name: &'n str, version: impl RangeBounds<&'v str> ) -> Result<LibInfo,pkg_config::Error> {
    let meta = load_toml( "clib.toml" ).ok().expect( "clib.toml should exist in crate clib." );
    let meta: Meta = toml::from_str( &meta ).ok().expect( "clib.toml should be valid toml file." );
    let name = meta.alias.get( name ).map( |s| s.as_str() ).unwrap_or( name );

    #[cfg( target_os = "freebsd" )]
    env::set_var( "PKG_CONFIG_ALLOW_CROSS", "1" );

    probe( name, version ).map( |lib| {
        let mut headers = Vec::new();
        let mut include_paths = Vec::new();
        probe_headers( name, &mut headers, &mut include_paths );

        lib.include_paths.iter().for_each( |path| {
            include_paths.push( path.to_str().expect( UTF8_PATH ).to_owned() );
        });

        LibInfo {
            link_paths    : lib.link_paths.iter().map( |path| path.to_str().expect( UTF8_PATH ).to_owned() ).collect(),
            include_paths ,
            headers       ,
        }
    })
}

fn get_includedir( pkg_name: &str ) -> String {
    let exe = env::var( "PKG_CONFIG" ).unwrap_or_else( |_| "pkg-config".to_owned() );
    let mut cmd = Command::new( exe );
    cmd.args( &[ pkg_name, "--variable", "includedir" ]);

    let output = cmd.output().expect( &format!( "`pkg-config {} --variable includedir` should run successfully.", pkg_name ));
    std::str::from_utf8( output.stdout.as_slice() ).expect( "pkg-config should generate utf8-compatible output." )
        .trim_end().to_owned()
}

fn probe_headers( pkg_name: &str, headers: &mut Vec<String>, include_dirs: &mut Vec<String> ) {
    load_spec( pkg_name ).map( |spec| {
        let include_dir = get_includedir( &pkg_name );
        let spec: Spec = toml::from_str( &spec ).ok().expect( &format!( "{} should be valid toml file.", pkg_name ));

        spec.header.files.iter().for_each( |file|
            headers.push( Path::new( &include_dir ).join( file ).to_str().expect( UTF8_PATH ).to_string() ));

        spec.header.import.map( |import| import.iter().for_each( |pkg| probe_headers( pkg, headers, include_dirs )));
        spec.header.import_dir.map( |import_dir| import_dir.iter().for_each( |pkg| {
            include_dirs.push( get_includedir( pkg ))
        }));
    });
}

pub fn fold_lib_info<'n,'v,'l>( name: &'n str, version: impl RangeBounds<&'v str>, lib_info_all: &'l mut LibInfo )
    -> Result<(), pkg_config::Error>
{
    probe_library( name, version )
    .map( |lib_info| {
        lib_info.link_paths.into_iter().for_each( |path| { lib_info_all.link_paths.push( path ); });
        lib_info.include_paths.into_iter().for_each( |path| { lib_info_all.include_paths.push( path ); });
        lib_info.headers.into_iter().for_each( |path| { lib_info_all.headers.push( path ); });
    })
}

fn generate_nothing() {
    let out_path = PathBuf::from( env::var( "OUT_DIR" ).expect( "$OUT_DIR should exist." ));
    File::create( out_path.join( "bindings.rs" )).expect( "an empty bindings.rs generated." );
}

fn main() {
    #[allow( unused_mut )]
    let mut pkgs = HashSet::<&str>::new();

    #[cfg( feature = "libcurl" )]
    pkgs.insert( "libcurl" );

    #[cfg( feature = "liblzma" )]
    pkgs.insert( "liblzma" );

    #[cfg( feature = "sqlite3" )]
    pkgs.insert( "sqlite3" );

    #[cfg( feature = "tcl86" )]
    pkgs.insert( "tcl86" );

    #[cfg( feature = "tk86" )]
    pkgs.insert( "tk86" );

    #[cfg( feature = "x11" )]
    pkgs.insert( "x11" );

    #[cfg( feature = "zlib" )]
    pkgs.insert( "zlib" );

    pkgs.extend( extra_libs().split(' ') );

    if pkgs.is_empty() {
        generate_nothing();
        return;
    }

    let mut lib_info_all = LibInfo::default();

    pkgs.iter().for_each( |pkg| if !pkg.is_empty() {
        match env::var( &format!( "CLIB_{}_MIN_VER", pkg.to_uppercase() )) {
            Ok( min_ver ) => {
                fold_lib_info( pkg, min_ver.as_str().., &mut lib_info_all ).ok()
            },
            Err( _ ) => fold_lib_info( pkg, .., &mut lib_info_all ).ok(),
        };
    });

    if lib_info_all.headers.is_empty() {
        generate_nothing();
        return;
    }

    let mut builder = Builder::default()
        .generate_comments( false )
    ;

    for header in lib_info_all.headers.iter() {
        builder = builder.header( header );
    }
    for path in lib_info_all.include_paths.iter() {
        let opt = format!( "-I{}", path );
        builder = builder.clang_arg( &opt );
    }

    let bindings = builder.generate().expect( "bindgen builder constructed." );
    let out_path = PathBuf::from( env::var( "OUT_DIR" ).expect( "$OUT_DIR should exist." ));
    bindings.write_to_file( out_path.join( "bindings.rs" )).expect( "bindings.rs generated." );
}
