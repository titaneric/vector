require 'ostruct'

class Hash
  def delete!(key)
    delete(key) { |key| raise("Key does not exist: #{key.inspect}") }
  end

  def flatten
    each_with_object({}) do |(k, v), h|
      if v.is_a? Hash
        v.flatten.map do |h_k, h_v|
          h["#{k}.#{h_k}"] = h_v
        end
      else
        h[k] = v
      end
    end
  end

  def to_query(*args)
    to_param(*args).gsub("[]", "").gsub("%5B%5D", "")
  end

  def to_struct(&block)
    new_hash = {}

    each do |key, val|
      new_hash[key] =
        if block_given?
          yield(key, val)
        else
          val
        end
    end

    AccessibleHash.new(new_hash)
  end

  def to_struct_with_name(constructor)
    to_struct do |key, hash|
      constructor.new(hash.merge({"name" => key}))
    end
  end
end
