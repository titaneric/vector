require_relative "generator"

class ConfigSpecificationGenerator < Generator
  attr_reader :schema,
    :global_example_generator

  def initialize(schema)
    @schema = schema
    @global_example_generator = OptionsExampleGenerator.new(schema.options.to_h.values.sort)
  end

  def generate
    <<~EOF
    #                                    __   __  __  
    #                                    \\ \\ / / / /
    #                                     \\ V / / /
    #                                      \\_/  \\/
    #
    #                                    V E C T O R
    #
    #                            Configuration Specification
    #
    # ------------------------------------------------------------------------------
    # Website: https://vectorproject.io
    # Docs: https://docs.vectorproject.io
    # Community: https://vectorproject.io/community
    # Repo: https://github.com/timberio/vector
    # ------------------------------------------------------------------------------
    # The file contains a full specification for the `vector.toml` configuration
    # file. It follows the TOML format and includes all options, types, and
    # possible values.
    #
    # More info on Vector's configuration can be found at:
    # https://docs.vectorproject.io/usage/configuration

    # ------------------------------------------------------------------------------
    # Global
    # ------------------------------------------------------------------------------
    # Global options are relevant to Vector as a whole and apply to global behavior.
    #
    # Documentation: https://docs.vectorproject.io/usage/configuration
    #{global_example_generator.generate("", :spec)}

    # ------------------------------------------------------------------------------
    # Sources
    # ------------------------------------------------------------------------------
    # Sources specify data sources and are responsible for ingesting data into
    # Vector.
    #
    # Documentation: https://docs.vectorproject.io/usage/configuration/sources
    #{source_examples}

    # ------------------------------------------------------------------------------
    # Transforms
    # ------------------------------------------------------------------------------
    # Transforms parse, structure, and enrich events.
    #
    # Documentation: https://docs.vectorproject.io/usage/configuration/transforms
    #{transform_examples}

    # ------------------------------------------------------------------------------
    # Sinks
    # ------------------------------------------------------------------------------
    # Sinks batch or stream data out of Vector.
    #
    # Documentation: https://docs.vectorproject.io/usage/configuration/sinks
    #{sink_examples}
    EOF
  end

  private
    def source_examples
      schema.sources.to_h.values.sort.collect do |source|
        generator = OptionsExampleGenerator.new(source.options.to_h.values.sort)
        generator.generate("sources.#{source.name}", :spec)
      end.join("\n\n")
    end

    def transform_examples
      schema.transforms.to_h.values.sort.collect do |transform|
        generator = OptionsExampleGenerator.new(transform.options.to_h.values.sort)
        generator.generate("transforms.#{transform.name}", :spec)
      end.join("\n\n")
    end

    def sink_examples
      schema.sinks.to_h.values.sort.collect do |sink|
        generator = OptionsExampleGenerator.new(sink.options.to_h.values.sort)
        generator.generate("sinks.#{sink.name}", :spec)
      end.join("\n\n")
    end
end