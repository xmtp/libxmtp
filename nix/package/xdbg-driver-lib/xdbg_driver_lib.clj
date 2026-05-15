#!/usr/bin/env bb
(ns xdbg-driver-lib
  (:require [cheshire.core :as json]
            [clojure.string :as str]
            [clojure.tools.cli :refer [parse-opts]]
            [xdbg-driver-lib.git :as git]
            [xdbg-driver-lib.versions :as v]
            [xdbg-driver-lib.xdbg :as xdbg]))

(defn usage []
  (println "Usage: xdbg-driver-lib <subcommand> [args...]")
  (println)
  (println "Subcommands:")
  (println "  bootstrap-fetch")
  (println "      Fetch release branches + nightly tags into the local repo.")
  (println "      Idempotent. pick-versions calls this implicitly, so")
  (println "      direct invocation is only needed if you want to pre-warm")
  (println "      the repo without computing a plan.")
  (println "  pick-versions [--sample-size N] [--no-bootstrap]")
  (println "      Run bootstrap-fetch then print a plan. --no-bootstrap")
  (println "      skips the fetch if the caller has already done it.")
  (println "  last-two-stable-branches")
  (println "  nightly-tag-candidates")
  (println "  flake-ref <kind> <sha>")
  (println "  sandbox-env-args")
  (println "  probe <mode> <kind> <sha>")
  (println "  supports-flag <kind> <sha> <flag>")
  (println "  supports-subcommand <kind> <sha> <subcommand>"))

(defn- run-bootstrap-fetch! []
  (let [{:keys [nightly-tags-fetched]} (git/bootstrap-fetch)]
    (binding [*out* *err*]
      (if (zero? nightly-tags-fetched)
        (println "::warning::no nightly tags matched; nightly sampling will be empty")
        (println (format "Fetching %d nightly tags" nightly-tags-fetched))))))

(defn -main [& args]
  (let [sub (first args)
        rest-args (rest args)]
    (case sub
      "bootstrap-fetch"
      (run-bootstrap-fetch!)

      "pick-versions"
      (let [{:keys [options errors]}
            (parse-opts rest-args
                        [[nil "--sample-size N" "Number of nightly samples"
                          :default 3
                          :parse-fn #(Long/parseLong %)
                          :validate [#(>= % 0) "Must be non-negative"]]
                         [nil "--no-bootstrap" "Skip the bootstrap-fetch call"
                          :default false]])]
        (when (seq errors)
          (binding [*out* *err*]
            (doseq [e errors] (println e)))
          (System/exit 2))
        (when-not (:no-bootstrap options)
          (run-bootstrap-fetch!))
        (println (json/generate-string (v/pick-versions options)
                                       {:pretty true})))

      "last-two-stable-branches"
      (doseq [b (v/last-two-stable-branches)]
        (println b))

      "nightly-tag-candidates"
      (doseq [[date sha7] (v/nightly-tag-candidates)]
        (println (str date "\t" sha7)))

      "flake-ref"
      (let [[kind sha] rest-args]
        (println (xdbg/flake-ref kind sha)))

      "sandbox-env-args"
      (doseq [[k v] (xdbg/sandbox-env-args)]
        (println (str k "=" v)))

      "probe"
      (let [[mode kind sha] rest-args
            {:keys [status stderr exit]} (xdbg/probe mode kind sha)]
        (case status
          :ok (System/exit 0)
          :eval-failed
          (do (binding [*out* *err*] (when (seq stderr) (print stderr) (flush)))
              (System/exit (or exit 2)))
          :build-failed
          (do (binding [*out* *err*] (when (seq stderr) (print stderr) (flush)))
              (System/exit 3))
          ;; Defensive default — surface unknown statuses as a clear
          ;; error rather than an uncaught IllegalArgumentException.
          (do (binding [*out* *err*]
                (println (str "probe: unexpected status " status)))
              (System/exit 4))))

      "supports-flag"
      (let [[kind sha flag] rest-args]
        (System/exit (if (xdbg/supports-flag kind sha flag) 0 1)))

      "supports-subcommand"
      (let [[kind sha sub-cmd] rest-args]
        (System/exit (if (xdbg/supports-subcommand kind sha sub-cmd) 0 1)))

      ("--help" "-h" nil) (usage)

      (do (binding [*out* *err*]
            (println "unknown subcommand:" sub)
            (usage))
          (System/exit 2)))))

(when (= *file* (System/getProperty "babashka.file"))
  (apply -main *command-line-args*))
