
# Nonvolatile &emsp; [![Latest Version]][crates.io]

[Latest Version]: https://img.shields.io/crates/v/nonvolatile
[crates.io]: https://crates.io/crates/nonvolatile

**Nonvolatile is a library for storing persistent settings and configuration data out of the way. This library is still a work-in-progress, and not fully functional.**

## Example

'''rust
use nonvolatile::State;
use generic_error::*;

fn main() -> Result<()> {
	
	//crate a new state instance
	let mut state = State::load_else_create("foo")?;
	//set a variable
	state.set("var", "some value")?;
	
	//destroy the state variable
	drop(state);
	
	//create a new state instance
	let state = State::load_else_create("foo")?;
	//retrieve the previously set variable.
	println!("foo: {}", state.get("var").unwrap());  //"some value"	
	Ok(())
}
'''