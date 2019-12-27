require "ostruct"
require "toml-rb"

require_relative "metadata/batching_sink"
require_relative "metadata/exposing_sink"
require_relative "metadata/field"
require_relative "metadata/links"
require_relative "metadata/post"
require_relative "metadata/release"
require_relative "metadata/source"
require_relative "metadata/streaming_sink"
require_relative "metadata/transform"

# Object representation of the /.meta directory
#
# This represents the /.meta directory in object form. Sub-classes represent
# each sub-component.
class Metadata
  class << self
    def load!(meta_dir, docs_root, pages_root)
      metadata = {}

      Dir.glob("#{meta_dir}/**/*.toml").each do |file|
        hash = TomlRB.load_file(file)
        metadata.deep_merge!(hash)
      end

      new(metadata, docs_root, pages_root)
    end
  end

  attr_reader :blog_posts,
  :env_vars,
    :installation,
    :links,
    :log_fields,
    :metric_fields,
    :options,
    :testing,
    :posts,
    :releases,
    :sinks,
    :sources,
    :team,
    :transforms

  def initialize(hash, docs_root, pages_root)
    @installation = OpenStruct.new()
    @log_fields = Field.build_struct(hash["log_fields"] || {})
    @metric_fields = Field.build_struct(hash["metric_fields"] || {})
    @options = Option.build_struct(hash.fetch("options"))
    @releases = OpenStruct.new()
    @sinks = OpenStruct.new()
    @sources = OpenStruct.new()
    @transforms = OpenStruct.new()
    @testing = Option.build_struct(hash.fetch("testing"))

    # installation

    installation_hash = hash.fetch("installation")
    @installation.containers = installation_hash.fetch("containers").collect { |h| OpenStruct.new(h) }
    @installation.downloads = installation_hash.fetch("downloads").collect { |h| OpenStruct.new(h) }
    @installation.operating_systems = installation_hash.fetch("operating_systems").collect { |h| OpenStruct.new(h) }
    @installation.package_managers = installation_hash.fetch("package_managers").collect { |h| OpenStruct.new(h) }

    # posts

    @posts ||=
      Dir.glob("#{POSTS_ROOT}/**/*.md").collect do |path|
        Post.new(path)
      end.sort

    # releases

    release_versions =
      hash.fetch("releases").collect do |version_string, _release_hash|
        Version.new(version_string)
      end

    hash.fetch("releases").collect do |version_string, release_hash|
      version = Version.new(version_string)

      last_version =
        release_versions.
          select { |other_version| other_version < version }.
          sort.
          last

      last_date = last_version && hash.fetch("releases").fetch(last_version.to_s).fetch("date").to_date

      release_hash["version"] = version_string
      release = Release.new(release_hash, last_version, last_date, @posts)
      @releases.send("#{version_string}=", release)
    end

    # sources

    hash["sources"].collect do |source_name, source_hash|
      source_hash["name"] = source_name
      source_hash["posts"] = posts.select { |post| post.source?(source_name) }
      source = Source.new(source_hash)
      @sources.send("#{source_name}=", source)
    end

    # transforms

    hash["transforms"].collect do |transform_name, transform_hash|
      transform_hash["name"] = transform_name
      transform_hash["posts"] = posts.select { |post| post.transform?(transform_name) }
      transform = Transform.new(transform_hash)
      @transforms.send("#{transform_name}=", transform)
    end

    # sinks

    hash["sinks"].collect do |sink_name, sink_hash|
      sink_hash["name"] = sink_name
      sink_hash["posts"] = posts.select { |post| post.sink?(sink_name) }

      sink =
        case sink_hash.fetch("egress_method")
        when "batching"
          BatchingSink.new(sink_hash)
        when "exposing"
          ExposingSink.new(sink_hash)
        when "streaming"
          StreamingSink.new(sink_hash)
        end

      @sinks.send("#{sink_name}=", sink)
    end

    # links

    @links = Links.new(hash.fetch("links"), docs_root, pages_root)

    # env vars

    @env_vars = Option.build_struct(hash["env_vars"] || {})

    components.each do |component|
      component.env_vars.to_h.each do |key, val|
        @env_vars.send("#{key}=", val)
      end
    end

    # team

    @team =
      hash.fetch("team").collect do |member|
        OpenStruct.new(member)
      end
  end

  def components
    @components ||= sources_list + transforms_list + sinks_list
  end

  def downloads(arch: nil, os: nil, package_manager: nil, type: nil)
    downloads = installation.downloads
    downloads = downloads.select { |d| d.arch && d.arch.downcase == arch.to_s.downcase } if arch
    downloads = downloads.select { |d| d.os && d.os.downcase == os.to_s.downcase } if os
    downloads = downloads.select { |d| d.package_manager && d.package_manager.downcase == package_manager.to_s.downcase } if package_manager
    downloads = downloads.select { |d| d.type && d.type.downcase == type.to_s.downcase } if type
    downloads
  end

  def env_vars_list
    @env_vars_list ||= env_vars.to_h.values.sort
  end

  def latest_patch_releases
    version = Version.new("#{latest_version.major}.#{latest_version.minor}.0")

    releases_list.select do |release|
      release.version >= version
    end
  end

  def latest_release
    @latest_release ||= releases_list.last
  end

  def latest_version
    @latest_version ||= latest_release.version
  end

  def log_fields_list
    @log_fields_list ||= log_fields.to_h.values.sort
  end

  def metric_fields_list
    @metric_fields_list ||= metric_fields.to_h.values.sort
  end

  def newer_releases(release)
    releases_list.select do |other_release|
      other_release > release
    end
  end

  def new_post
    return @new_post if defined?(@new_post)

    @new_post ||=
      begin
        last_post = posts.last

        if (Date.today - last_post.date) <= 30
          last_post
        else
          nil
        end
      end
  end

  def new_release
    return @new_post if defined?(@new_post)

    @new_post ||=
      begin
        last_release = releases.releases_list.last

        if (Date.today - last_release.date) <= 30
          last_release
        else
          nil
        end
      end
  end

  def post_tags
    @post_tags ||= posts.collect(&:tags).flatten.uniq
  end

  def platforms
    @platforms ||= installation.containers +
      installation.operating_systems +
      installation.package_managers
  end

  def previous_minor_releases(release)
    releases_list.select do |other_release|
      other_release.version < release.version &&
        other_release.version.major != release.version.major &&
        other_release.version.minor != release.version.minor
    end
  end

  def releases_list
    @releases_list ||= @releases.to_h.values.sort
  end

  def relesed_versions
    releases
  end

  def sinks_list
    @sinks_list ||= sinks.to_h.values.sort
  end

  def sources_list
    @sources_list ||= sources.to_h.values.sort
  end

  def to_h
    {
      installation: installation.deep_to_h,
      latest_post: posts.last.deep_to_h,
      latest_release: latest_release.deep_to_h,
      posts: posts.deep_to_h,
      post_tags: post_tags,
      releases: releases.deep_to_h,
      sources: sources.deep_to_h,
      team: team.deep_to_h,
      transforms: transforms.deep_to_h,
      sinks: sinks.deep_to_h
    }
  end

  def transforms_list
    @transforms_list ||= transforms.to_h.values.sort
  end
end
