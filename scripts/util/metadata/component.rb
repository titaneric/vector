#encoding: utf-8

require_relative "option"

class Component
  DELIVERY_GUARANTEES = ["at_least_once", "best_effort"].freeze
  EVENT_TYPES = ["log", "metric"].freeze
  OPERATING_SYSTEMS = ["Linux", "MacOS", "Windows"].freeze

  include Comparable

  attr_reader :beta,
    :common,
    :env_vars,
    :function_category,
    :id,
    :name,
    :operating_systems,
    :options,
    :posts,
    :requirements,
    :resources,
    :title,
    :type,
    :unsupported_operating_systems

  def initialize(hash)
    @beta = hash["beta"] == true
    @common = hash["common"] == true
    @env_vars = Option.build_struct(hash["env_vars"] || {})
    @function_category = hash.fetch("function_category")
    @name = hash.fetch("name")
    @posts = hash.fetch("posts")
    @requirements = hash["requirements"]
    @title = hash.fetch("title")
    @type ||= self.class.name.downcase
    @id = "#{@name}_#{@type}"
    @options = Option.build_struct(hash["options"] || {})

    # Operating Systems

    if hash["only_operating_systems"]
      @operating_systems = hash["only_operating_systems"]
    elsif hash["except_operating_systems"]
      @operating_systems = OPERATING_SYSTEMS - hash["except_operating_systems"]
    else
      @operating_systems = OPERATING_SYSTEMS
    end

    @unsupported_operating_systems = OPERATING_SYSTEMS - @operating_systems

    # Resources

    @resources =
      (hash.delete("resources") || []).collect do |resource_hash|
        OpenStruct.new(resource_hash)
      end

    # Default options

    @options.type =
      Option.new({
        "name" => "type",
        "description" => "The component type. This is a required field that tells Vector which component to use. The value _must_ be `#{name}`.",
        "enum" => {
          name => "The name of this component"
        },
        "required" => true,
        "type" => "string"
      })

    if type != "source"
      @options.inputs =
        Option.new({
          "name" => "inputs",
          "description" => "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [configuration][docs.configuration] for more info.",
          "examples" => [["my-source-id"]],
          "required" => true,
          "type" => "[string]"
        })
    end
  end

  def <=>(other)
    name <=> other.name
  end

  def advanced_relevant?
    options_list.any?(&:advanced?)
  end

  def beta?
    beta == true
  end

  def common?
    common == true
  end

  def context_options
    options_list.select(&:context?)
  end

  def env_vars_list
    @env_vars_list ||= env_vars.to_h.values.sort
  end

  def event_types
    types = []

    if respond_to?(:input_types)
      types += input_types
    end

    if respond_to?(:output_types)
      types += output_types
    end

    types.uniq
  end

  def options_list
    @options_list ||= options.to_h.values.sort
  end

  def partition_options
    options_list.select(&:partition_key?)
  end

  def sink?
    type == "sink"
  end

  def source?
    type == "source"
  end

  def specific_options_list
    options_list.select do |option|
      !["type", "inputs"].include?(option.name)
    end
  end

  def status
    beta? ? "beta" : "prod-ready"
  end

  def templateable_options
    options_list.select(&:templateable?)
  end

  def to_h
    {
      beta: beta?,
      delivery_guarantee: (respond_to?(:delivery_guarantee, true) ? delivery_guarantee : nil),
      description: description,
      event_types: event_types,
      function_category: (respond_to?(:function_category, true) ? function_category : nil),
      id: id,
      name: name,
      operating_systems: (transform? ? [] : operating_systems),
      service_providers: (respond_to?(:service_providers, true) ? service_providers : nil),
      status: status,
      type: type,
      unsupported_operating_systems: unsupported_operating_systems
    }
  end

  def transform?
    type == "transform"
  end
end
