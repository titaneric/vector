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

require_relative "setup"

#
# Requires
#

require_relative "generate/post_processors/link_definer"
require_relative "generate/post_processors/option_referencer"
require_relative "generate/post_processors/section_sorter"
require_relative "generate/post_processors/toml_syntax_switcher"
require_relative "generate/templates"

require_relative "generate/core_ext/hash"
require_relative "generate/core_ext/string"

#
# Functions
#

def doc_valid?(file_or_dir)
  parts = file_or_dir.split("#", 2)
  path = DOCS_ROOT + parts[0]
  anchor = parts[1]

  if File.exists?(path)
    if !anchor.nil?
      file_path = File.directory?(path) ? "#{path}/README.md" : path
      content = File.read(file_path)
      headings = content.scan(/\n###?#?#? (.*)\n/).flatten.uniq
      anchors = headings.collect(&:parameterize)
      anchors.include?(anchor)
    else
      true
    end
  else
    false
  end
end

def link_valid?(value)
  if value.start_with?("/")
    doc_valid?(value)
  else
    url_valid?(value)
  end
end

def post_process(content, doc, links)
  content = PostProcessors::TOMLSyntaxSwitcher.switch!(content)
  content = PostProcessors::SectionSorter.sort!(content)
  content = PostProcessors::OptionReferencer.reference!(content)
  content = PostProcessors::LinkDefiner.define!(content, doc, links)
  content
end

def url_valid?(url)
  uri = URI.parse(url)
  req = Net::HTTP.new(uri.host, uri.port)
  req.open_timeout = 500
  req.read_timeout = 1000
  req.ssl_timeout = 1000
  req.use_ssl = true if uri.scheme == 'https'
  path = uri.path == "" ? "/" : uri.path

  begin
    res = req.request_head(path)
    res.code.to_i != 404
  rescue Errno::ECONNREFUSED
    return false
  end
end

#
# Header
#

title("Generating files...")

#
# Setup
#

metadata =
  begin
    Metadata.load!(META_ROOT, DOCS_ROOT)
  rescue Exception => e
    error!(e.message)
  end

templates = Templates.new(TEMPLATES_DIR, metadata)

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

Dir.glob("#{TEMPLATES_DIR}/**/*.erb", File::FNM_DOTMATCH).
  to_a.
  select { |path| !templates.partial?(path) }.
  each do |template_path|
    target_file = template_path.gsub(/^#{TEMPLATES_DIR}\//, "").gsub(/\.erb$/, "")
    target_path = "#{ROOT_DIR}/#{target_file}"
    content = templates.render(target_file)
    content = post_process(content, target_path, metadata.links)


    # Create the file if it does not exist
    if !File.exists?(target_path)
      File.open(target_path, "w") {}
    end

    current_content = File.read(target_path)

    if current_content != content
      action = false ? "Will be changed" : "Changed"
      say("#{action} - #{target_file}", color: :green)
      File.write(target_path, content)
    else
      action = false ? "Will not be changed" : "Not changed"
      say("#{action} - #{target_file}", color: :blue)
    end
  end

#
# Post process individual docs
#

title("Post processing generated files...")

docs = Dir.glob("#{DOCS_ROOT}/**/*.md").to_a
docs = docs + ["#{ROOT_DIR}/README.md"]
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

#
# Check URLs
#

title("Checking URLs...")

check_urls = get("Would you like to check & verify URLs?", ["y", "n"]) == "y"

if check_urls
  Parallel.map(metadata.links.values.to_a.sort, in_threads: 30) do |id, value|
    if !link_valid?(value)
      error!(
        <<~EOF
        Link invalid!

          #{value}

        Please make sure this path or URL exists.
        EOF
      )
    else
      say("Valid - #{id} - #{value}", color: :green)
    end
  end
end