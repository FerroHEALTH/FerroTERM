- The console's settings name the configuration file they were read from
  wherever they are resolved: `Config::load` keeps the path it read and
  `Config::resolve` carries it, where only the command line's own start
  filled it in before.
