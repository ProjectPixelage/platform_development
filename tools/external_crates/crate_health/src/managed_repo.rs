// Copyright (C) 2024 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    collections::BTreeSet,
    fs::{create_dir, read_dir, remove_file, write},
    os::unix::fs::symlink,
    path::Path,
    process::Command,
    str::from_utf8,
};

use anyhow::{anyhow, Context, Result};
use glob::glob;
use google_metadata::GoogleMetadata;
use itertools::Itertools;
use license_checker::find_licenses;
use name_and_version::{NameAndVersionMap, NameAndVersionRef, NamedAndVersioned};
use rooted_path::RootedPath;
use semver::Version;
use spdx::Licensee;

use crate::{
    android_bp::cargo_embargo_autoconfig,
    copy_dir,
    crate_collection::CrateCollection,
    crate_type::Crate,
    license::{most_restrictive_type, update_module_license_files},
    managed_crate::ManagedCrate,
    pseudo_crate::{CargoVendorClean, CargoVendorDirty, PseudoCrate},
    SuccessOrError,
};

pub struct ManagedRepo {
    path: RootedPath,
}

impl ManagedRepo {
    pub fn new(path: RootedPath) -> ManagedRepo {
        ManagedRepo { path }
    }
    fn pseudo_crate(&self) -> PseudoCrate<CargoVendorDirty> {
        PseudoCrate::new(self.path.join("pseudo_crate").unwrap())
    }
    fn contains(&self, crate_name: &str) -> bool {
        self.managed_dir_for(crate_name).abs().exists()
    }
    fn managed_dir(&self) -> RootedPath {
        self.path.join("crates").unwrap()
    }
    fn managed_dir_for(&self, crate_name: &str) -> RootedPath {
        self.managed_dir().join(crate_name).unwrap()
    }
    fn legacy_dir_for(&self, crate_name: &str) -> RootedPath {
        self.path.with_same_root("external/rust/crates").unwrap().join(crate_name).unwrap()
    }
    fn new_cc(&self) -> CrateCollection {
        CrateCollection::new(self.path.root())
    }
    fn managed_crate_for(
        &self,
        crate_name: &str,
    ) -> Result<ManagedCrate<crate::managed_crate::New>> {
        Ok(ManagedCrate::new(Crate::from(self.managed_dir_for(crate_name))?))
    }
    pub fn all_crate_names(&self) -> Result<Vec<String>> {
        let mut managed_dirs = Vec::new();
        for entry in read_dir(self.managed_dir())? {
            let entry = entry?;
            if entry.path().is_dir() {
                managed_dirs.push(
                    entry.file_name().into_string().map_err(|e| {
                        anyhow!("Failed to convert {} to string", e.to_string_lossy())
                    })?,
                );
            }
        }
        Ok(managed_dirs)
    }
    pub fn migration_health(
        &self,
        crate_name: &str,
        verbose: bool,
        unpinned: bool,
    ) -> Result<Version> {
        if self.contains(crate_name) {
            return Err(anyhow!("Crate {} already exists in {}/crates", crate_name, self.path));
        }

        let mut cc = self.new_cc();
        cc.add_from(self.legacy_dir_for(crate_name).rel())?;
        if cc.map_field().len() != 1 {
            return Err(anyhow!(
                "Expected a single crate version for {}, but found {}. Crates with multiple versions are not supported yet.",
                crate_name,
                cc.map_field().len()
            ));
        }
        let krate = cc.map_field().values().next().unwrap();
        println!("Found {} v{} in {}", krate.name(), krate.version(), krate.path());

        let mut healthy_self_contained = true;
        if krate.is_migration_denied() {
            println!("This crate is on the migration denylist");
            healthy_self_contained = false;
        }

        let mc = ManagedCrate::new(Crate::from(self.legacy_dir_for(crate_name))?).as_legacy();
        if !mc.android_bp().abs().exists() {
            println!("There is no Android.bp file in {}", krate.path());
            healthy_self_contained = false;
        }
        if !mc.cargo_embargo_json().abs().exists() {
            println!("There is no cargo_embargo.json file in {}", krate.path());
            healthy_self_contained = false;
        }
        if healthy_self_contained {
            let mc = mc.stage()?;
            if !mc.cargo_embargo_success() {
                println!("cargo_embargo execution did not succeed for {}", mc.staging_path(),);
                if verbose {
                    println!(
                        "stdout:\n{}\nstderr:\n{}",
                        from_utf8(&mc.cargo_embargo_output().stdout)?,
                        from_utf8(&mc.cargo_embargo_output().stderr)?,
                    );
                }
                healthy_self_contained = false;
            } else if !mc.android_bp_unchanged() {
                println!(
                    "Running cargo_embargo on {} produced changes to the Android.bp file",
                    mc.staging_path()
                );
                if verbose {
                    println!("{}", from_utf8(&mc.android_bp_diff().stdout)?);
                }
                healthy_self_contained = false;
            }
        }

        if !healthy_self_contained {
            println!("Crate {} is UNHEALTHY", crate_name);
            return Err(anyhow!("Crate {} is unhealthy", crate_name));
        }

        let pseudo_crate = self.pseudo_crate();
        if unpinned {
            pseudo_crate.cargo_add_unpinned(krate)?;
        } else {
            pseudo_crate.cargo_add(krate)?;
        }
        let pseudo_crate = pseudo_crate.vendor()?;

        let mc = ManagedCrate::new(Crate::from(self.legacy_dir_for(crate_name))?)
            .stage(&pseudo_crate)?;

        pseudo_crate.remove(krate.name())?;

        let version = mc.vendored_version().clone();
        if mc.android_version() != mc.vendored_version() {
            println!(
                "Source and destination versions are different: {} -> {}",
                mc.android_version(),
                mc.vendored_version()
            );
        }
        if !mc.patch_success() {
            println!("Patches did not apply successfully to the migrated crate");
            if verbose {
                for output in mc.patch_output() {
                    if !output.1.status.success() {
                        println!(
                            "Failed to apply {}\nstdout:\n{}\nstderr:\n:{}",
                            output.0,
                            from_utf8(&output.1.stdout)?,
                            from_utf8(&output.1.stderr)?
                        );
                    }
                }
            }
        }
        if !mc.cargo_embargo_success() {
            println!("cargo_embargo execution did not succeed for the migrated crate");
        } else if !mc.android_bp_unchanged() {
            println!("Running cargo_embargo for the migrated crate produced changes to the Android.bp file");
            if verbose {
                println!("{}", from_utf8(&mc.android_bp_diff().stdout)?);
            }
        }

        let mut diff_cmd = Command::new("diff");
        diff_cmd.args(["-u", "-r", "-w", "--no-dereference"]);
        if !verbose {
            diff_cmd.arg("-q");
        }
        let diff_status = diff_cmd
            .args(IGNORED_FILES.iter().map(|ignored| format!("--exclude={}", ignored)))
            .args(["-I", r#"default_team: "trendy_team_android_rust""#])
            .arg(mc.android_crate_path().rel())
            .arg(mc.staging_path().rel())
            .current_dir(self.path.root())
            .spawn()?
            .wait()?;
        if !diff_status.success() {
            println!(
                "Found differences between {} and {}",
                mc.android_crate_path(),
                mc.staging_path()
            );
        }
        if verbose {
            println!("All diffs:");
            Command::new("diff")
                .args(["-u", "-r", "-w", "-q", "--no-dereference"])
                .arg(mc.android_crate_path().rel())
                .arg(mc.staging_path().rel())
                .current_dir(self.path.root())
                .spawn()?
                .wait()?;
        }

        if !mc.patch_success() || !mc.cargo_embargo_success() || !mc.android_bp_unchanged() {
            println!("Crate {} is UNHEALTHY", crate_name);
            return Err(anyhow!("Crate {} is unhealthy", crate_name));
        }

        if diff_status.success() {
            println!("Crate {} is healthy", crate_name);
            return Ok(version);
        }

        if unpinned {
            println!("The crate was added with an unpinned version, and diffs were found which must be inspected manually");
            return Ok(version);
        }

        println!("Crate {} is UNHEALTHY", crate_name);
        Err(anyhow!("Crate {} is unhealthy", crate_name))
    }
    pub fn migrate<T: AsRef<str>>(
        &self,
        crates: Vec<T>,
        verbose: bool,
        unpinned: &BTreeSet<String>,
    ) -> Result<()> {
        let pseudo_crate = self.pseudo_crate();
        for crate_name in &crates {
            let crate_name = crate_name.as_ref();
            let version =
                self.migration_health(crate_name, verbose, unpinned.contains(crate_name))?;
            let src_dir = self.legacy_dir_for(crate_name);

            let monorepo_crate_dir = self.managed_dir();
            if !monorepo_crate_dir.abs().exists() {
                create_dir(monorepo_crate_dir)?;
            }
            copy_dir(src_dir, self.managed_dir_for(crate_name))?;
            if unpinned.contains(crate_name) {
                pseudo_crate.cargo_add_unpinned(&NameAndVersionRef::new(crate_name, &version))?;
            } else {
                pseudo_crate.cargo_add(&NameAndVersionRef::new(crate_name, &version))?;
            }
        }

        self.regenerate(crates.iter(), false)?;

        for crate_name in &crates {
            let crate_name = crate_name.as_ref();
            let src_dir = self.legacy_dir_for(crate_name);
            for entry in glob(
                src_dir
                    .abs()
                    .join("*.bp")
                    .to_str()
                    .ok_or(anyhow!("Failed to convert path *.bp to str"))?,
            )? {
                remove_file(entry?)?;
            }
            remove_file(src_dir.join("cargo_embargo.json")?)?;
            let test_mapping = src_dir.join("TEST_MAPPING")?;
            if test_mapping.abs().exists() {
                remove_file(test_mapping)?;
            }
            write(
                src_dir.join("Android.bp")?,
                format!("// This crate has been migrated to {}.\n", self.path),
            )?;
        }

        Ok(())
    }
    pub fn import(&self, crate_name: &str) -> Result<()> {
        let (new_deps, pseudo_crate) = self.add_crate_and_dependencies(crate_name)?;

        for dep in &new_deps {
            println!("Sprinkling Android glitter on {}", dep);

            if self.contains(dep) {
                return Err(anyhow!(
                    "Crate {} already exists at {}",
                    dep,
                    self.managed_dir_for(dep)
                ));
            }
            if self.legacy_dir_for(dep).abs().exists() {
                return Err(anyhow!(
                    "Legacy crate {} already exists at {}",
                    dep,
                    self.legacy_dir_for(dep)
                ));
            }

            let vendored_dir = pseudo_crate.vendored_dir_for(dep)?;
            let managed_dir = self.managed_dir_for(dep);
            copy_dir(vendored_dir, &managed_dir)?;

            // TODO: Copy to a temp dir, because otherwise we might run cargo and create/modify Cargo.lock.
            // TODO: Maybe just write a default cargo_embargo.json if cargo_embargo fails horribly.
            // There is one pathological crate out there (unarray) where the version published
            // to crates.io doesn't compile, and cargo_embargo relies on at least being
            // able to compile successfully. In such case, we may need to do:
            //  write(managed_dir.abs().join("cargo_embargo.json"), "{}")?;
            cargo_embargo_autoconfig(&managed_dir)?
                .success_or_error()
                .context("Failed to generate cargo_embargo.json")?;

            let krate = Crate::from(managed_dir.clone())?;

            let licenses = find_licenses(krate.path().abs(), krate.name(), krate.license())?;

            if !licenses.unsatisfied.is_empty() && licenses.satisfied.is_empty() {
                let mut satisfied = false;
                // Sometimes multiple crates live in a single GitHub repo. A common case
                // is a crate with an associated proc_macro crate. In such cases, the individual
                // crates are in subdirectories with license files at root of the repo, and
                // the license files don't get distributed with the crates.
                // So, if we didn't find a license file, try to guess the URL of the appropriate
                // license file and download it. This is incredibly hacky, and only supports
                // the most common case, which is LICENSE-APACHE.
                if licenses.unsatisfied.len() == 1 {
                    let req = licenses.unsatisfied.first().unwrap();
                    if let Some(repository) = krate.repository() {
                        if *req == Licensee::parse("Apache-2.0").unwrap().into_req() {
                            let url = format!("{}/master/LICENSE-APACHE", repository);
                            let body = reqwest::blocking::get(
                                url.replace("github.com", "raw.githubusercontent.com"),
                            )?
                            .text()?;
                            write(krate.path().abs().join("LICENSE"), body)?;
                            let patch_dir = krate.path().abs().join("patches");
                            create_dir(&patch_dir)?;
                            let output = Command::new("diff")
                                .args(["-u", "/dev/null", "LICENSE"])
                                .current_dir(krate.path().abs())
                                .output()?;
                            write(patch_dir.join("LICENSE.patch"), output.stdout)?;
                            satisfied = true;
                        }
                    }
                }
                if !satisfied {
                    return Err(anyhow!(
                        "Could not find license files for all licenses. Missing {}",
                        licenses.unsatisfied.iter().join(", ")
                    ));
                }
            }

            // If there's a single applicable license file, symlink it to LICENSE.
            if licenses.satisfied.len() == 1 && licenses.unsatisfied.is_empty() {
                let license_file = krate.path().join("LICENSE")?;
                if !license_file.abs().exists() {
                    symlink(
                        licenses.satisfied.iter().next().unwrap().1.file_name().unwrap(),
                        license_file,
                    )?;
                }
            }

            update_module_license_files(&krate.path().abs(), &licenses)?;

            let metadata = GoogleMetadata::init(
                krate.path().join("METADATA")?,
                krate.name(),
                krate.version().to_string(),
                krate.description(),
                most_restrictive_type(&licenses),
            )?;
            metadata.write()?;

            // Workaround. Our logic for crate health assumes the crate isn't healthy if there's
            // no Android.bp. So create an empty one.
            write(krate.path().abs().join("Android.bp"), "")?;

            // TODO: Create TEST_MAPPING
        }

        self.regenerate(new_deps.iter(), true)?;

        Ok(())
    }
    pub fn regenerate<T: AsRef<str>>(
        &self,
        crates: impl Iterator<Item = T>,
        update_metadata: bool,
    ) -> Result<()> {
        let pseudo_crate = self.pseudo_crate().vendor()?;
        for crate_name in crates {
            let mc = self.managed_crate_for(crate_name.as_ref())?;
            // TODO: Don't give up if there's a failure.
            mc.regenerate(update_metadata, &pseudo_crate)?;
        }

        pseudo_crate.regenerate_crate_list()?;

        Ok(())
    }
    pub fn stage<T: AsRef<str>>(&self, crates: impl Iterator<Item = T>) -> Result<()> {
        let pseudo_crate = self.pseudo_crate().vendor()?;
        for crate_name in crates {
            let mc = self.managed_crate_for(crate_name.as_ref())?.stage(&pseudo_crate)?;
            // TODO: Don't give up if there's a failure.
            mc.check_staged()?;
        }
        Ok(())
    }
    pub fn preupload_check(&self, files: &[String]) -> Result<()> {
        let pseudo_crate = self.pseudo_crate().vendor()?;
        let deps = pseudo_crate.deps().keys().cloned().collect::<BTreeSet<_>>();

        let managed_dirs = self.all_crate_names()?.into_iter().collect();

        if deps != managed_dirs {
            return Err(anyhow!("Deps in pseudo_crate/Cargo.toml don't match directories in {}\nDirectories not in Cargo.toml: {}\nCargo.toml deps with no directory: {}",
                self.managed_dir(), managed_dirs.difference(&deps).join(", "), deps.difference(&managed_dirs).join(", ")));
        }

        let crate_list = pseudo_crate.read_crate_list()?;
        if deps.iter().ne(crate_list.iter()) {
            return Err(anyhow!("Deps in pseudo_crate/Cargo.toml don't match deps in crate-list.txt\nCargo.toml: {}\ncrate-list.txt: {}",
                deps.iter().join(", "), crate_list.iter().join(", ")));
        }

        let changed_android_crates = files
            .iter()
            .filter_map(|file| {
                let path = Path::new(file);
                let components = path.components().collect::<Vec<_>>();
                if path.starts_with("crates/") && components.len() > 2 {
                    Some(components[1].as_os_str().to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();

        for crate_name in changed_android_crates {
            println!("Checking {}", crate_name);
            let mc = self.managed_crate_for(&crate_name)?.stage(&pseudo_crate)?;
            mc.diff_staged()?;
        }
        Ok(())
    }
    // TODO: Run "cargo tree" for android targets as well. By default
    // it runs it for the host target.
    fn add_crate_and_dependencies(
        &self,
        crate_name: &str,
    ) -> Result<(BTreeSet<String>, PseudoCrate<CargoVendorClean>)> {
        let mut cc = self.new_cc();
        cc.add_from("external/rust/crates")?;
        let unmigrated_crates =
            cc.map_field().keys().map(|nv| nv.name().to_string()).collect::<BTreeSet<_>>();

        let pseudo_crate = self.pseudo_crate().vendor()?;
        let migrated_crates = pseudo_crate.deps().keys().cloned().collect::<BTreeSet<_>>();

        let mut pending_deps = BTreeSet::from([crate_name.to_string()]);
        let mut added_deps = BTreeSet::new();
        while !pending_deps.is_empty() {
            let cur_dep = pending_deps.pop_first().unwrap();
            println!("Adding {}", cur_dep);
            let pseudo_crate = self.pseudo_crate();
            pseudo_crate.cargo_add_unversioned(&cur_dep)?;
            // TODO: Try not to do "cargo vendor" so often.
            let pseudo_crate = pseudo_crate.vendor()?;
            added_deps.insert(cur_dep.clone());
            for new_dep in pseudo_crate.deps_of(&cur_dep)? {
                if !added_deps.contains(&new_dep)
                    && !migrated_crates.contains(&new_dep)
                    && !unmigrated_crates.contains(&new_dep)
                {
                    println!("  Depends on {}", new_dep);
                    pending_deps.insert(new_dep);
                }
            }
        }
        Ok((added_deps, self.pseudo_crate().vendor()?))
    }
    pub fn fix_licenses(&self) -> Result<()> {
        let mut cc = self.new_cc();
        cc.add_from(self.managed_dir().rel())?;

        for krate in cc.map_field().values() {
            println!("{} = \"={}\"", krate.name(), krate.version());
            let state = find_licenses(krate.path().abs(), krate.name(), krate.license())?;
            if !state.unsatisfied.is_empty() {
                println!("{:?}", state);
            } else {
                // For now, just update MODULE_LICENSE_*
                update_module_license_files(&krate.path().abs(), &state)?;
            }
        }

        Ok(())
    }
    pub fn fix_metadata(&self) -> Result<()> {
        let mut cc = self.new_cc();
        cc.add_from(self.managed_dir().rel())?;

        for krate in cc.map_field().values() {
            println!("{} = \"={}\"", krate.name(), krate.version());
            let mut metadata = GoogleMetadata::try_from(krate.path().join("METADATA")?)?;
            metadata.set_version_and_urls(krate.name(), krate.version().to_string())?;
            metadata.migrate_archive();
            metadata.migrate_homepage();
            metadata.remove_deprecated_url();
            metadata.write()?;
        }

        Ok(())
    }
    pub fn recontextualize_patches<T: AsRef<str>>(
        &self,
        crates: impl Iterator<Item = T>,
    ) -> Result<()> {
        for crate_name in crates {
            let mc = self.managed_crate_for(crate_name.as_ref())?;
            mc.recontextualize_patches()?;
        }
        Ok(())
    }
}

// Files that are ignored when migrating a crate to the monorepo.
static IGNORED_FILES: &[&str] = &[
    ".appveyor.yml",
    ".bazelci",
    ".bazelignore",
    ".bazelrc",
    ".bazelversion",
    ".buildkite",
    ".cargo",
    ".cargo-checksum.json",
    ".cargo_vcs_info.json",
    ".circleci",
    ".cirrus.yml",
    ".clang-format",
    ".clang-tidy",
    ".clippy.toml",
    ".clog.toml",
    ".clog.toml",
    ".codecov.yaml",
    ".codecov.yml",
    ".editorconfig",
    ".envrc",
    ".gcloudignore",
    ".gdbinit",
    ".git",
    ".git-blame-ignore-revs",
    ".git-ignore-revs",
    ".gitallowed",
    ".gitattributes",
    ".github",
    ".gitignore",
    ".idea",
    ".ignore",
    ".istanbul.yml",
    ".mailmap",
    ".md-inc.toml",
    ".mdl-style.rb",
    ".mdlrc",
    ".pylintrc",
    ".pylintrc-examples",
    ".pylintrc-tests",
    ".reuse",
    ".rspec",
    ".rustfmt.toml",
    ".shellcheckrc",
    ".standard-version",
    ".tarpaulin.toml",
    ".tokeignore",
    ".travis.yml",
    ".versionrc",
    ".vim",
    ".vscode",
    ".yapfignore",
    ".yardopts",
    "BUILD",
    "Cargo.lock",
    "Cargo.lock.saved",
    "Cargo.toml.orig",
    "OWNERS",
    // Deprecated config file for rules.mk.
    "cargo2rulesmk.json",
    // cargo_embargo intermediates.
    "Android.bp.orig",
    "cargo.metadata",
    "cargo.out",
    "target.tmp",
];
