# [[file:~/Workspace/Programming/gosh-rs/runner/runners.note::*makefile][makefile:1]]
default: install

install: stow
	stow --verbose --adopt --no-folding --target ~/ pkg
uninstall:
	stow --verbose --target ~/ --delete pkg

stow:
	which stow
# makefile:1 ends here
