#!/usr/bin/env ruby

# generate.rb
#
# SUMMARY
#
#   A simple script that generates files across the Vector repo. This is used
#   for documentation, config examples, etc. The source templates are located
#   in /scripts/generate/templates/* and the results are placed in their
#   respective root directories.
#
#   See the README.md in the generate folder for more details.

#
# Setup
#

# Changes into the release-prepare directory so that we can load the
# Bundler dependencies. Unfortunately, Bundler does not provide a way
# load a Gemfile outside of the cwd.
Dir.chdir "scripts/generate"

#
# Requires
#

require "rubygems"
require "bundler"
Bundler.require(:default)

require_relative "util/core_ext/object"
require_relative "util/printer"
require_relative "generate/post_processors/link_checker"
require_relative "generate/post_processors/option_referencer"
require_relative "generate/post_processors/section_sorter"
require_relative "generate/post_processors/toml_syntax_switcher"
require_relative "generate/templates"

require_relative "generate/core_ext/hash"
require_relative "generate/core_ext/string"

#
# Includes
#

include Printer

#
# Functions
#

def post_process(content, doc, links)
  content = PostProcessors::TOMLSyntaxSwitcher.switch!(content)
  content = PostProcessors::SectionSorter.sort!(content)
  content = PostProcessors::OptionReferencer.reference!(content)
  content = PostProcessors::LinkChecker.check!(content, doc, links)
  content
end

#
# Constants
#

VECTOR_ROOT = File.join(Dir.pwd.split(File::SEPARATOR)[0..-3])

DOCS_ROOT = File.join(VECTOR_ROOT, "docs")
META_ROOT = File.join(VECTOR_ROOT, ".meta")
TEMPLATES_DIR = "#{Dir.pwd}/templates"
VECTOR_DOCS_HOST = "https://docs.vector.dev"

#
# Header
#

title("Generating files...")

#
# Options
#

check_urls = get("Would you like to check & verify URLs?", ["y", "n"]) == "y"

#
# Setup
#

metadata = Metadata.load(META_ROOT, check_urls: check_urls)
templates = Templates.new(metadata)

#
# Create missing component templates
#

metadata.components.each do |component|
  # Configuration templates
  template_path = "#{TEMPLATES_DIR}/docs/usage/configuration/#{component.type.pluralize}/#{component.name}.md.erb"

  if !File.exists?(template_path)
    contents = templates.component_default(component)
    File.open(template_path, 'w+') { |file| file.write(contents) }
  end
end

#
# Render templates
#

Dir.glob("templates/**/*.erb", File::FNM_DOTMATCH).
  to_a.
  select { |path| !templates.partial?(path) }.
  each do |template_path|
    content = templates.render(template_path.gsub(/^templates\//, "").gsub(/\.erb$/, ""))
    target = template_path.gsub(/^templates\//, "#{VECTOR_ROOT}/").gsub(/\.erb$/, "")
    content = post_process(content, target, metadata.links)

    # Create the file if it does not exist
    if !File.exists?(target)
      File.open(target, "w") {}
    end

    current_content = File.read(target)

    if current_content != content
      action = false ? "Will be changed" : "Changed"
      say("#{action} - #{target.gsub("../../", "")}", color: :green)
      File.write(target, content)
    else
      action = false ? "Will not be changed" : "Not changed"
      say("#{action} - #{target.gsub("../../", "")}", color: :blue)
    end
  end

#
# Post process individual docs
#

title("Post processing generated files...")

docs = Dir.glob("#{DOCS_ROOT}/**/*.md").to_a
docs = docs + ["#{VECTOR_ROOT}/README.md"]
docs = docs - ["#{DOCS_ROOT}/SUMMARY.md"]
docs.each do |doc|
  content = File.read(doc)
  if content.include?("THIS FILE IS AUTOGENERATED")
    say("Skipped - #{doc}", color: :blue)
  else
    content = post_process(content, doc, metadata.links)
    File.write(doc, content)
    say("Processed - #{doc}", color: :green)
  end
end
