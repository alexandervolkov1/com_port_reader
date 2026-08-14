use std::{
    env,
    error::Error,
    ffi::{OsStr, OsString},
    fmt, fs, io,
    path::{Path, PathBuf},
};

pub const STARTUP_SCRIPT_FILE_NAME: &str = "startup.lua";
pub const ACTIVE_PROFILE_FILE_NAME: &str = "active-profile.txt";
const APPLICATION_STATE_DIRECTORY: &str = "state";
const CONFIG_OPTION: &str = "--config";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationPaths {
    application_directory: PathBuf,
    startup_script: PathBuf,
    profile_directory: PathBuf,
}

impl ApplicationPaths {
    pub fn discover() -> Result<Self, ApplicationPathsError> {
        let application_directory = default_application_directory()?;

        let config_path = config_path_from_args(env::args_os().skip(1))?;

        let startup_script = match config_path {
            Some(path) => make_absolute(path)?,

            None => remembered_startup_script(&application_directory)
                .unwrap_or_else(|| application_directory.join(STARTUP_SCRIPT_FILE_NAME)),
        };

        Self::new(application_directory, startup_script)
    }

    pub fn from_startup_script(
        startup_script: impl Into<PathBuf>,
    ) -> Result<Self, ApplicationPathsError> {
        let startup_script = make_absolute(startup_script.into())?;

        let application_directory =
            startup_script
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| {
                    ApplicationPathsError::new(format!(
                        "Startup script path '{}' has no parent directory",
                        startup_script.display(),
                    ))
                })?;

        Self::new(application_directory, startup_script)
    }

    pub fn with_startup_script(
        &self,
        startup_script: impl Into<PathBuf>,
    ) -> Result<Self, ApplicationPathsError> {
        Self::new(self.application_directory.clone(), startup_script.into())
    }

    fn new(
        application_directory: impl Into<PathBuf>,
        startup_script: impl Into<PathBuf>,
    ) -> Result<Self, ApplicationPathsError> {
        let application_directory = make_absolute(application_directory.into())?;
        let startup_script = make_absolute(startup_script.into())?;

        let profile_directory =
            startup_script
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| {
                    ApplicationPathsError::new(format!(
                        "Startup script path '{}' has no parent directory",
                        startup_script.display(),
                    ))
                })?;

        Ok(Self {
            application_directory,
            startup_script,
            profile_directory,
        })
    }

    pub fn application_directory(&self) -> &Path {
        &self.application_directory
    }

    pub fn startup_script(&self) -> &Path {
        &self.startup_script
    }

    pub fn profile_directory(&self) -> &Path {
        &self.profile_directory
    }

    pub fn resolve_profile(&self, path: impl AsRef<Path>) -> PathBuf {
        resolve_from(&self.profile_directory, path.as_ref())
    }

    pub fn resolve_data(&self, path: impl AsRef<Path>) -> PathBuf {
        resolve_from(&self.application_directory, path.as_ref())
    }

    pub fn remember_active_profile(&self) -> io::Result<()> {
        let state_directory = self.application_directory.join(APPLICATION_STATE_DIRECTORY);

        fs::create_dir_all(&state_directory)?;

        let state_file = state_directory.join(ACTIVE_PROFILE_FILE_NAME);

        fs::write(state_file, format!("{}\n", self.startup_script.display()))
    }
}

fn active_profile_state_path(application_directory: &Path) -> PathBuf {
    application_directory
        .join(APPLICATION_STATE_DIRECTORY)
        .join(ACTIVE_PROFILE_FILE_NAME)
}

fn remembered_startup_script(application_directory: &Path) -> Option<PathBuf> {
    let state_file = active_profile_state_path(application_directory);

    let source = fs::read_to_string(state_file).ok()?;

    let value = source.trim();

    if value.is_empty() {
        return None;
    }

    let path = PathBuf::from(value);

    let path = if path.is_absolute() {
        path
    } else {
        application_directory.join(path)
    };

    path.is_file().then_some(path)
}

fn resolve_from(directory: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        directory.join(path)
    }
}

#[cfg(debug_assertions)]
fn default_application_directory() -> Result<PathBuf, ApplicationPathsError> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf())
}

#[cfg(not(debug_assertions))]
fn default_application_directory() -> Result<PathBuf, ApplicationPathsError> {
    let executable = env::current_exe().map_err(|error| {
        ApplicationPathsError::new(format!("Failed to determine executable path: {error}",))
    })?;

    executable.parent().map(Path::to_path_buf).ok_or_else(|| {
        ApplicationPathsError::new(format!(
            "Executable path '{}' has no parent directory",
            executable.display(),
        ))
    })
}

fn config_path_from_args(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Option<PathBuf>, ApplicationPathsError> {
    let mut arguments = arguments.into_iter();
    let mut config_path = None;

    while let Some(argument) = arguments.next() {
        if argument == OsStr::new(CONFIG_OPTION) {
            let path = arguments.next().ok_or_else(|| {
                ApplicationPathsError::new(format!("{CONFIG_OPTION} requires a file path",))
            })?;

            set_config_path(&mut config_path, PathBuf::from(path))?;

            continue;
        }

        if let Some(argument) = argument.to_str()
            && let Some(path) = argument.strip_prefix("--config=")
        {
            if path.is_empty() {
                return Err(ApplicationPathsError::new("--config requires a file path"));
            }

            set_config_path(&mut config_path, PathBuf::from(path))?;

            continue;
        }

        return Err(ApplicationPathsError::new(format!(
            "Unknown command-line argument '{}'",
            argument.to_string_lossy(),
        )));
    }

    Ok(config_path)
}

fn set_config_path(
    destination: &mut Option<PathBuf>,
    path: PathBuf,
) -> Result<(), ApplicationPathsError> {
    if destination.is_some() {
        return Err(ApplicationPathsError::new(
            "Startup configuration path was specified \
             more than once",
        ));
    }

    *destination = Some(path);

    Ok(())
}

fn make_absolute(path: PathBuf) -> Result<PathBuf, ApplicationPathsError> {
    if path.as_os_str().is_empty() {
        return Err(ApplicationPathsError::new(
            "Startup script path cannot be empty",
        ));
    }

    if path.is_absolute() {
        return Ok(path);
    }

    let current_directory = env::current_dir().map_err(|error| {
        ApplicationPathsError::new(format!(
            "Failed to determine current directory: \
                 {error}",
        ))
    })?;

    Ok(current_directory.join(path))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationPathsError {
    message: String,
}

impl ApplicationPathsError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ApplicationPathsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ApplicationPathsError {}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{ApplicationPaths, config_path_from_args, remembered_startup_script};

    #[test]
    fn resolves_relative_path_from_startup_directory() {
        let directory = std::env::temp_dir().join("com_port_reader_path_test");

        let startup_script = directory.join("startup.lua");

        let paths = ApplicationPaths::from_startup_script(&startup_script).unwrap();

        assert_eq!(paths.startup_script(), startup_script,);

        assert_eq!(
            paths.resolve_profile("scripts/emulator/sine.lua",),
            directory.join("scripts/emulator/sine.lua"),
        );
    }

    #[test]
    fn leaves_absolute_path_unchanged() {
        let directory = std::env::temp_dir().join("com_port_reader_path_test");

        let paths = ApplicationPaths::from_startup_script(directory.join("startup.lua")).unwrap();

        let absolute = std::env::temp_dir().join("external/model.lua");

        assert_eq!(paths.resolve_profile(&absolute), absolute);
    }

    #[test]
    fn parses_config_option() {
        let result = config_path_from_args([
            OsString::from("--config"),
            OsString::from("configurations/test.lua"),
        ])
        .unwrap();

        assert_eq!(result, Some(PathBuf::from("configurations/test.lua",)),);
    }

    #[test]
    fn parses_inline_config_option() {
        let result =
            config_path_from_args([OsString::from("--config=configurations/test.lua")]).unwrap();

        assert_eq!(result, Some(PathBuf::from("configurations/test.lua",)),);
    }

    #[test]
    fn rejects_missing_config_path() {
        let result = config_path_from_args([OsString::from("--config")]);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_duplicate_config_options() {
        let result = config_path_from_args([
            OsString::from("--config"),
            OsString::from("first.lua"),
            OsString::from("--config"),
            OsString::from("second.lua"),
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn root_directory_is_startup_parent() {
        let directory = std::env::temp_dir().join("com_port_reader_path_test");

        let paths = ApplicationPaths::from_startup_script(directory.join("startup.lua")).unwrap();

        assert_eq!(paths.profile_directory(), Path::new(&directory),);
    }

    #[test]
    fn preserves_application_directory_when_profile_changes() {
        let application_directory = std::env::temp_dir().join("com_port_reader_application");

        let profile_directory = std::env::temp_dir().join("com_port_reader_profile");

        let paths =
            ApplicationPaths::from_startup_script(application_directory.join("startup.lua"))
                .unwrap();

        let paths = paths
            .with_startup_script(profile_directory.join("experiment.lua"))
            .unwrap();

        assert_eq!(paths.application_directory(), application_directory,);

        assert_eq!(paths.profile_directory(), profile_directory,);

        assert_eq!(
            paths.resolve_profile("lua_scripts/process.lua"),
            profile_directory.join("lua_scripts/process.lua"),
        );

        assert_eq!(
            paths.resolve_data("logs"),
            application_directory.join("logs"),
        );
    }

    #[test]
    fn remembers_active_profile() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let application_directory = std::env::temp_dir().join(format!(
            "com_port_reader_profile_state_{}_{}",
            std::process::id(),
            unique,
        ));

        let profile_directory = application_directory.join("profiles");

        fs::create_dir_all(&profile_directory).unwrap();

        let startup_script = profile_directory.join("experiment.lua");

        fs::write(&startup_script, "return {}").unwrap();

        let paths = ApplicationPaths::new(&application_directory, &startup_script).unwrap();

        paths.remember_active_profile().unwrap();

        assert_eq!(
            remembered_startup_script(&application_directory,),
            Some(startup_script),
        );

        let _ = fs::remove_dir_all(application_directory);
    }

    #[test]
    fn ignores_missing_remembered_profile() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let application_directory = std::env::temp_dir().join(format!(
            "com_port_reader_missing_profile_{}_{}",
            std::process::id(),
            unique,
        ));

        let state_directory = application_directory.join("state");

        fs::create_dir_all(&state_directory).unwrap();

        fs::write(
            state_directory.join("active-profile.txt"),
            application_directory
                .join("missing.lua")
                .display()
                .to_string(),
        )
        .unwrap();

        assert_eq!(remembered_startup_script(&application_directory,), None,);

        let _ = fs::remove_dir_all(application_directory);
    }
}
