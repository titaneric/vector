#encoding: utf-8

require_relative "component"
require_relative "output"

class Sink < Component
  attr_reader :delivery_guarantee,
    :egress_method,
    :input_types,
    :healthcheck,
    :output,
    :service_limits_short_link,
    :tls,
    :write_to_description

  def initialize(hash)
    @type = "sink"
    super(hash)

    @delivery_guarantee = hash.fetch("delivery_guarantee")
    @egress_method = hash.fetch("egress_method")
    @healthcheck = hash.fetch("healthcheck")
    @input_types = hash.fetch("input_types")
    @service_limits_short_link = hash["service_limits_short_link"]
    @write_to_description = hash.fetch("write_to_description")

    if @write_to_description.strip[-1] == "."
      raise("#{self.class.name}#write_to_description cannot not end with a period")
    end

    # output

    if hash["output"]
      @output = Output.new(hash["output"])
    end
  end

  def batching?
    egress_method == "batching"
  end

  def description
    @description ||= "#{plural_write_verb.humanize} #{input_types.to_sentence} events to #{write_to_description}."
  end

  def exposing?
    egress_method == "exposing"
  end

  def healthcheck?
    healthcheck == true
  end

  def plural_write_verb
    case egress_method
    when "batching"
      "batches"
    when "exposing"
      "exposes"
    when "streaming"
      "streams"
    else
      raise("Unhandled egress_method: #{egress_method.inspect}")
    end
  end

  def streaming?
    egress_method == "streaming"
  end

  def write_verb
    case egress_method
    when "batching"
      "batch and flush"
    when "exposing"
      "expose"
    when "streaming"
      "stream"
    else
      raise("Unhandled egress_method: #{egress_method.inspect}")
    end
  end
end
