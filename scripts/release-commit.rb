#!/usr/bin/env ruby

# release-commit.rb
#
# SUMMARY
#
#   Commits and tags the pending release

require_relative "setup"

#
# Functions
#

def bump_cargo_version(version)
  # Cargo.toml
  content = File.read("#{ROOT_DIR}/Cargo.toml")
  new_content = bump_version(content, version)
  File.write("#{ROOT_DIR}/Cargo.toml", new_content)

  # Cargo.lock
  content = File.read("#{ROOT_DIR}/Cargo.lock")
  new_content = bump_version(content, version)
  File.write("#{ROOT_DIR}/Cargo.lock", new_content)
end

def bump_version(content, version)
  content.sub(
    /name = "vector"\nversion = "([a-z0-9.-]*)"\n/,
    "name = \"vector\"\nversion = \"#{version}\"\n"
  )
end

def release_exists?(release)
  errors = `git rev-parse v#{release.version} 2>&1 >/dev/null`
  errors == ""
end

#
# Commit
#

metadata = Metadata.load!(META_ROOT, DOCS_ROOT, PAGES_ROOT)
release = metadata.latest_release

if release_exists?(release)
  error!(
    <<~EOF
    It looks like release v#{release.version} has already been released. A tag for this release already exists.

    This command will only release the latest release. If you're trying to release from an older major or minor version, you must do so from that branch.
    EOF
  )
else
  title("Committing and tagging release")

  bump_cargo_version(release.version)

  success("Bumped the version in Cargo.toml & Cargo.lock to #{release.version}")

  branch_name = "#{release.version.major}.#{release.version.minor}"

  commands =
    <<~EOF
    git add . -A
    git commit -sam 'chore: Prepare v#{release.version} release'
    git push origin master
    git tag -a v#{release.version} -m "v#{release.version}"
    git push origin v#{release.version}
    git branch v#{branch_name}
    git push origin v#{branch_name}
    EOF

  commands.chomp!

  status = `git status --short`.chomp!

  words =
    <<~EOF
    We'll be releasing v#{release.version} with the following commands:

    #{commands.indent(2)}

    Your current `git status` is:

    #{status.indent(2)}

    Proceed to execute the above commands?
    EOF

  if Printer.get(words, ["y", "n"]) == "n"
    Printer.error!("Ok, I've aborted. Please re-run this command when you're ready.")
  end

  commands.chomp.split("\n").each do |command|
    system(command)

    if !$?.success?
      error!(
        <<~EOF
        Command failed!

          #{command}

        Produced the following error:

          #{$?.inspect}
        EOF
      )
    end
  end
end
