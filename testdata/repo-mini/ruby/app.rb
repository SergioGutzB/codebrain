require_relative 'services/greeter'

module App
  class Runner
    def call(name)
      Services::Greeter.new.greet(name)
    end
  end
end
