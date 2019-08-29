
use whoami;
use whoami::Platform::{Linux, Windows};
use serde::{Serialize, Deserialize};
use serde_yaml;
use std::fs::{
	copy,
	create_dir_all, 
	rename, 
	metadata,
	read_to_string, 
	OpenOptions,
	remove_file,
	remove_dir_all,
	canonicalize,
	is_file,
};
use std::path::Path;
use std::collections::HashMap;
use std::env;
use std::io::Write;
use std::process;
use std::thread;
use std::time;
use std::ops::Drop;
use rand;
use sysinfo;
use generic_error::{Result, GenErr, GenericError};
use fs_util::copy_dir;


#[cfg(test)]
mod tests;


#[derive(Serialize, Deserialize, Debug)]
pub struct State {
	name: String,
	path: String,
	identifier: String,
	lockfile_path: String,
	manifest_path: String,
	tmp_manifest_path: String,
	items: HashMap<String, String>,
	preserved: HashMap<String, String>,
}


enum WhoOwns {
	Me,
	Other,
	Nobody,
}


fn build_var_path(var: &str, sub_dir: &str) -> Result<String> {
	let s = env::var(var)?;
	Ok(format!("{}/{}", s, sub_dir))
}


fn get_storage_dir() -> Result<String> {
	match whoami::platform() {
		Linux => {
			build_var_path("HOME", ".local/rust_nonvolatile")
		},
		Windows => {
			build_var_path("appdata", "rust_nonvolatile")
		},
		_ => GenErr!("nonvolatile: {} not supported", whoami::platform()),
	}
}


fn get_state_id() -> String {
	format!("{}-{}", process:id(), rand_random::<u32>());
}


fn match_state_id(my_id: &str, read_id: &str) -> WhoOwns {
	if my_id == read_id {
		return WhoOwns::Me;
	}
	let parts = read_id.split("-").collect();
	let (pid_str, rid_str) = match parts.len() {
		2 => (parts[0], parts[1]),
		_ => return WhoOwns::Nobody,
	};
	let read_pid: u32 = match pid_str.parse() {
		Ok(pid) => pid,
		Err(e) => return WhoOwns::Nobody,
	};
	let mut system = sysinfo::System::new();
	system.refresh_processes();
	for (other_pid, _proc) in system.get_process_list() {
		if other_pid == read_pid {
			return WhoOwns::Other;
		}
	}
	WhoOwns::Nobody
}


fn get_lock_acquired(lockfile_path: &str, state_id: &str) -> Result<bool> {
	if !is_file(lockfile_path) {
		return Ok(false);
	}
	let read_id = read_to_string(lockfile_path)?;
	match match_state_id(state_id, read_id) {
		WhoOwns::Me => return Ok(true),
		WhoOwns::Other => return GenErr!("lockfile {} already owned by state {}", lockfile_path, read_id),
		WhoOwns::Nobody => return Ok(false),
	}
}


fn acquire_dir(lockfile_path: &str, state_id: &str) -> Result<()> {
	match get_lock_acquired(lockfile_path, state_id) {
		Ok(true) => return Ok(()),
		Ok(false) => (),
		Err(e) => return e,
	};
	let file = OpenOptions::new().write(true).create(true).open(lockfile_path)?;
	write!(file, "{}", state_id)?;
	drop file;
	thread::sleep(time::Duration::new(0, 1000));
	match get_lock_acquired(lockfile_path, state_id) {
		Ok(true) => Ok(()),
		Ok(false) => GenErr!("Nobody owns the lock, but I still managed to fail to acquire it!"),
		Err(e) => Err(e),
	}
}


impl State {

	fn write_manifest(&self) -> Result<()> {
		let mut file = OpenOptions::new().write(true).create(true).open(&self.tmp_manifest_path)?;
		let data = serde_yaml::to_vec(self)?;
		file.write(&data)?;
		rename(&self.tmp_manifest_path, &self.manifest_path)?;
		Ok(())
	}
	
	
	pub fn set(&mut self, var: &str, value: &str) -> Result<()> {
		if self.preserved.contains_key(var) {
			return GenErr!("nonvolatile: can't set a variable with the same name as a preserved file/folder");
		}
		let _ = self.items.insert(String::from(var), String::from(value));
		self.write_manifest()
	}
	
	
	pub fn get(&self, var: &str) -> Option<String> {
		let item = self.items.get(var)?;
		Some(String::from(item))
	}
	
	
	pub fn delete(&mut self, name: &str) -> Result<()> {
		let _ = self.items.remove(name);
		if let Some(_) = self.preserved.remove(name) {
			let path = format!("{}/{}", &self.path, name);
			remove_file(&path)?;
			remove_dir_all(&path)?;
		}
		self.write_manifest()
	}
	
	
	pub fn new(name: &str) -> Result<State> {
		let dir = get_storage_dir()?;
		let path = format!("{}/{}", &dir, name);
		create_dir_all(&path)?;
		let items: HashMap<String, String> = HashMap::new();
		let preserved: HashMap<String, String> = HashMap::new();
		
		let state_id = get_state_id();
		let lockfile_path = Path::new(format!("{}/{}", path, "~rust_nonvolatile.lock"));
		
		acquire_dir(&lockfile_path, &state_id)?;
		
		let state = State {
			name: String::from(name),
			path: path.clone(),
			identifier: state_id,
			lockfile_path: lockfile_path,
			manifest_path: format!("{}/{}", &path, ".manifest"),
			tmp_manifest_path: format!("{}/{}", &path, ".manifest_tmp"),
			items: items,
			preserved: preserved,
		};
		State::write_manifest(&state)?;
		Ok(state)
	}
	
	
	pub fn load(name: &str) -> Result<State> {
		let dir = get_storage_dir()?;		
		let path = format!("{}/{}", &dir, name);
		let manifest_path = format!("{}/{}", &path, ".manifest");
		
		let state_id = get_state_id();
		let lockfile_path = Path::new(format!("{}/{}", path, "~rust_nonvolatile.lock"));
		
		acquire_dir(&lockfile_path, &state_id)?;
		
		let data = read_to_string(&manifest_path)?;
		let mut state: State = serde_yaml::from_str(&data)?;
		
		state.state_id = state_id;
		state.lockfile_path = lockfile_path;
		
		Ok(state)
	}
	
	
	pub fn load_else_create(name: &str) -> Result<State> {
		State::load(name).or_else(|_| State::new(name))
	}
	
	
	pub fn destroy_state(name: &str) {
		if let Ok(dir) = get_storage_dir() {
			let path = format!("{}/{}", dir, name);
			let _ = remove_dir_all(path);
		} 
	}
	
	
	pub fn has(&self, item: &str) -> bool {
		self.items.contains_key(item) || self.preserved.contains_key(item)
	}
	
	
	fn preserve(&mut self, path: &str, name: &str) -> Result<()> {
		if self.items.contains_key(name) {
			return GenErr!("nonvolatile: can't preserve a file with the same name as a set variable");
		}
		let path = match canonicalize(path)?.to_str() {
			Some(p) => String::from(p),
			None => return GenErr!("nonvolatile preserve: failed to canonicalize path"),
		};
		let tmp_name = format!("tmp_{}", name);
		let tmp_dest = format!("{}/{}", &self.path, &tmp_name);
		let dest = format!("{}/{}", &self.path, name);
		if metadata(&path)?.is_dir() {
			copy_dir(&path, &tmp_dest)?;
		}
		else {
			copy(&path, &tmp_dest)?;
		}
		
		let _ = self.preserved.insert(String::from(name), String::from(path));
		self.write_manifest()?;
		
		rename(tmp_dest, dest)?;
		Ok(())
	}


	fn restore(&self, name: &str) -> Result<()> {
		let path = match self.preserved.get(name) {
			Some(p) => p,
			None => return GenErr!("Nothing by the name '{}' has been preserved", name),
		};
		self.restore_to(name, path)
	}
	
	
	fn restore_to(&self, name: &str, path: &str) -> Result<()> {
		if !self.preserved.contains_key(name) {
			return GenErr!("Nothing by the name '{}' has been preserved", name);
		}
		let preserved_path = format!("{}/{}", &self.path, name);
		if metadata(&preserved_path)?.is_dir() {
			copy_dir(&preserved_path, path)?;
		}
		else {
			copy(&preserved_path, path)?;
		}
		Ok(())
	}
}


impl Drop for State {
	fn drop(&mut self) {
		let _ = remove_file(self.lockfile_path);
	}
}
