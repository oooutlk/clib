use bindgen::Builder;
use pkg_config::{Config, Library};
use serde_derive::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    env,
    fmt::Debug,
    fs::{self, File},
    io::{self, Read, Write},
    ops::RangeBounds,
    path::{Path, PathBuf},
    process::Command,
    sync::Once,
};
use toml::{
    Value,
    value::Table,
};

#[derive( Debug, Deserialize )]
struct Spec {
    headers             : Vec<String>,
    dependencies        : Option<Vec<String>>,
    #[serde( rename = "header-dependencies" )]
    header_dependencies : Option<Vec<String>>,
}

fn probe<'n,'v>( name: &'n str, version: impl RangeBounds<&'v str> ) -> Result<Library, pkg_config::Error> {
    let mut cfg = Config::new();
    cfg.cargo_metadata( true ).range_version( version );
    Ok( cfg.probe( name )? )
}

fn read_file( path: impl AsRef<Path> ) -> Result<String, io::Error> {
    let mut file = File::open( path )?;
    let mut contents = String::new();
    file.read_to_string( &mut contents )?;
    Ok( contents )
}

fn load_toml( dir: impl AsRef<Path>, name: &str ) -> Option<Value> {
    let path = dir.as_ref().join( format!( "{}.toml", name ));
    read_file( path ).ok().map( |contents| toml::from_str( &contents ).unwrap() )
}

fn save_toml( dir: impl AsRef<Path> + Copy, name: &str, toml: &Value ) {
    fs::create_dir_all( dir ).ok();
    let path = dir.as_ref().join( format!( "{}.toml", name ));
    let mut file = File::create( path ).unwrap();
    file.write( toml.to_string().as_bytes() );
}

const UTF8_PATH: &'static str = "path should be valid UTF-8 string.";

#[derive( Debug, Default )]
pub struct LibInfo {
    link_paths    : Vec<String>,
    include_paths : Vec<String>,
    headers       : Vec<String>,

}

pub fn probe_library<'n,'v>( name: &'n str, version: impl RangeBounds<&'v str> ) -> Result<LibInfo,pkg_config::Error> {
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
    let spec = load_spec( pkg_name );
    let include_dir = get_includedir( &pkg_name );
    let spec: Spec = toml::from_str( &spec ).expect( &format!( "{} should be valid toml file.", pkg_name ));

    spec.headers.iter().for_each( |file|
        headers.push( Path::new( &include_dir ).join( file ).to_str().expect( UTF8_PATH ).to_string() ));

    spec.dependencies.map( |import| import.iter().for_each( |pkg| probe_headers( pkg, headers, include_dirs )));
    spec.header_dependencies.map( |import_dir| import_dir.iter().for_each( |pkg| {
        include_dirs.push( get_includedir( pkg ))
    }));
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
    let out_path = PathBuf::from( env::var("OUT_DIR").expect( "$OUT_DIR should exist." ));
    File::create( out_path.join( "bindings.rs" )).expect( "an empty bindings.rs generated." );
}

fn get_clib_out_dir() -> PathBuf {
    Path::new( &env::var("OUT_DIR").unwrap() )
        .join( "clib-0.2.0" )
}

fn save_spec( pkgs: &Value ) {
    if let Value::Table( table ) = pkgs {
        table.into_iter().for_each( |(name, value)| {
            let spec_dir = get_clib_out_dir();
            let saved_spec: Option<Value> = load_toml( &spec_dir, &name );
            match saved_spec {
                Some( saved_spec ) => if &saved_spec != value {
                    panic!( "[clib] got two different specs of {}.\nOne is \"{}\",\nthe other is \"{}\".",
                        name, saved_spec.to_string(), value.to_string()
                    );
                },
                None => save_toml( &spec_dir, &name, value ),
            }
        });
    } else {
        panic!( "{} is not a toml table", pkgs );
    }
}

fn load_spec( name: &str ) -> String {
    let spec_path = get_clib_out_dir().join( &format!( "{}.toml", name ));
    read_file( &spec_path ).expect( &format!( "{:?} should exist", spec_path ))
}

pub fn metabuild() {
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let cargo_manifest_dir = Path::new( &cargo_manifest_dir );
    let toml: Value = load_toml( &cargo_manifest_dir, "Cargo" ).unwrap();
    if let Some( package ) = toml.get("package") {
        if let Some( metadata ) = package.get("metadata") {
            if let Some( clib ) = metadata.get("clib") {
                if let Some( pkgs ) = clib.get("pkgs") {
                    println!("save_spec");
                    save_spec( pkgs );
                }
                if let Some( build_pkgs ) = clib.get("build-pkgs") {
                    build( build_pkgs );
                }
            }
        }
    }
}

fn build( pkgs: &Value ) {
    let pkgs = match pkgs {
        Value::Array( pkgs ) => pkgs,
        _ => panic!("`build-pkgs` should be list of strings."),
    };

    let mut lib_info_all = LibInfo::default();
    let out_dir = env::var("OUT_DIR").expect("$OUT_DIR should exist.");

    pkgs.iter().for_each( |pkg|
        if let Value::String( pkg ) = pkg {
            if !pkg.is_empty() {
                fold_lib_info( &pkg, .., &mut lib_info_all ).ok();
            }
        } else {
            panic!("`build-pkgs` should be list of strings.")
        }
    );

    if lib_info_all.headers.is_empty() {
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
    let out_path = PathBuf::from( out_dir );
    bindings.write_to_file( out_path.join( "bindings.rs" )).expect( "bindings.rs generated." );
}
