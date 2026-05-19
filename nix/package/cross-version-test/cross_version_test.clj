#!/usr/bin/env bb
;; cross-version-test — drive multiple xdbg builds against a shared
;; XVT_DB_ROOT to detect cross-version MLS regressions.
;;
;; Usage:
;;   cross-version-test run [--profile stable|nightly] [--sample-size N]
;;                          [-- xdbg-flags...]
;;
;; Most logic lives in xdbg-driver-lib (loaded via --classpath in the
;; wrapper). This script is the dispatcher + the cross-version-specific
;; phase loop.

(ns cross-version-test
  (:require [babashka.cli :as cli]
            [clojure.string :as str]
            [xdbg-driver-lib.driver :as d]
            [xdbg-driver-lib.git :as git]
            [xdbg-driver-lib.versions :as v]
            [xdbg-driver-lib.xdbg :as xdbg]))

(defn- finalize!
  [{:keys [results completed-count required-failure nightly-failure
           lenient-nightlies out-dir]}]
  (d/emit-stderr-table "cross-version-test" results)
  (let [summary-json (str out-dir "/summary.json")]
    (spit summary-json
          (d/format-summary-json
            {:results results
             :completed-count completed-count
             :required-failure required-failure
             :nightly-failure nightly-failure
             :lenient-nightlies lenient-nightlies}))
    (d/eputs (str "::notice::summary JSON: " summary-json)))
  (d/emit-github-step-summary
    {:test-name "cross-version-test"
     :results results :out-dir out-dir
     :completed-count completed-count
     :required-failure required-failure
     :nightly-failure nightly-failure
     :lenient-nightlies lenient-nightlies})
  (cond
    (< completed-count 2)
    (do (d/eputs (str "::error::run-sequence: insufficient cross-version coverage (completed="
                      completed-count "); need at least 2 successful healthcheck runs"))
        (System/exit 1))
    required-failure
    (do (d/eputs "::error::run-sequence: one or more required versions failed")
        (System/exit 1))
    (and nightly-failure (not lenient-nightlies))
    (do (d/eputs "::error::run-sequence: one or more nightlies failed (strict mode)")
        (System/exit 1))
    nightly-failure
    (d/eputs "::notice::run-sequence completed with nightly failures (lenient mode; HEAD + required versions OK)")
    :else
    (d/eputs "::notice::run-sequence completed without failures")))

(defn- handle-probe-ok
  "Probe returned 0 — check healthcheck support, run it, fold the result
   into the accumulator. Returns the next loop state as a vector
   [next-todo results completed-count required-failure? nightly-failure?]."
  [{:keys [env-extras out-dir]} entry rest-todo
   results completed-count required-failure? nightly-failure?]
  (let [{:keys [kind sha short branch label required?]} entry
        base-row {:short short :sha sha :kind kind
                  :branch branch :label label}]
    (d/run-xdbg-info env-extras kind sha "--version")
    (cond
      (not (xdbg/supports-subcommand kind sha "healthcheck"))
      (if required?
        (do (d/eputs (str "::error::xdbg@" short " required version (" branch
                          ") lacks 'healthcheck' subcommand; cannot continue"))
            [[] (-> results
                    (conj (assoc base-row :status "FAIL"))
                    (d/record-not-run-remaining rest-todo))
             completed-count true nightly-failure?])
        (do (d/eputs (str "::warning::xdbg@" short " nightly " label
                          " lacks 'healthcheck' subcommand; skipping"))
            ;; Skip is a capability gap, not a runtime failure; do not
            ;; flip nightly-failure? so strict-mode runs don't exit 1
            ;; when an old nightly simply lacks the subcommand.
            [rest-todo (conj results (assoc base-row :status "SKIP-NO-HEALTHCHECK"))
             completed-count required-failure? nightly-failure?]))

      :else
      (do
        (d/eputs (format "::group::healthcheck@%s (%s %s)" short kind label))
        (let [rc (d/run-xdbg-real
                   {:env-extras env-extras
                    :tmp-prefix "xvt-call-"
                    :base-dir out-dir}
                   kind sha "healthcheck")]
          (d/eputs "::endgroup::")
          (if (zero? rc)
            [rest-todo (conj results (assoc base-row :status "PASS"))
             (inc completed-count) required-failure? nightly-failure?]
            (let [results' (conj results (assoc base-row :status "FAIL"))]
              (if required?
                (do (d/eputs (str "::error::xdbg@" short " required version (" kind
                                  ") healthcheck failure (rc=" rc
                                  "); continuing to remaining entries"))
                    [rest-todo results' completed-count true nightly-failure?])
                (do (d/eputs (str "::warning::xdbg@" short " nightly " label
                                  " healthcheck failure (rc=" rc
                                  "); continuing so later versions (incl. HEAD) still run"))
                    [rest-todo results' completed-count required-failure? true])))))))))

(defn- run-sequence-body
  [env-extras parsed out-dir lenient-nightlies]
  (let [_ (d/eputs (str "::notice::Running " (count parsed) " versions"))]
    (d/emit-plan-table parsed)
    (loop [todo parsed
           results []
           completed-count 0
           required-failure? false
           nightly-failure? false]
      (if (empty? todo)
        (finalize! {:results results
                    :completed-count completed-count
                    :required-failure required-failure?
                    :nightly-failure nightly-failure?
                    :lenient-nightlies lenient-nightlies
                    :out-dir out-dir})

        (let [[entry & rest-todo] todo
              {:keys [kind sha short branch label required?]} entry
              base-row {:short short :sha sha :kind kind
                        :branch branch :label label}
              probe-mode (if required? "dry" "full")
              {:keys [status stderr]} (xdbg/probe probe-mode kind sha)]
          (case status
            :ok
            (let [[next-todo r cc rf nf]
                  (handle-probe-ok {:env-extras env-extras :out-dir out-dir}
                                   entry rest-todo
                                   results completed-count
                                   required-failure? nightly-failure?)]
              (recur next-todo r cc rf nf))

            :build-failed
            (do (when (seq stderr)
                  (binding [*out* *err*] (print stderr) (flush)))
                (if required?
                  (do (d/eputs (str "::error::run-sequence: required version " short
                                    " (" branch ") fails to build"))
                      (recur [] (-> results
                                    (conj (assoc base-row :status "FAIL"))
                                    (d/record-not-run-remaining rest-todo))
                             completed-count true nightly-failure?))
                  (do (d/eputs (str "::warning::xdbg@" short " nightly " label
                                    " fails to build; skipping"))
                      ;; Build skip is a capability gap, not a runtime
                      ;; test failure — don't flip nightly-failure?.
                      (recur rest-todo
                             (conj results (assoc base-row :status "SKIP-BUILD"))
                             completed-count required-failure? nightly-failure?))))

            :eval-failed
            (do (when (seq stderr)
                  (binding [*out* *err*] (print stderr) (flush)))
                (if required?
                  (do (d/eputs (str "::error::run-sequence: aborting on probe eval failure for "
                                    short))
                      (recur [] (-> results
                                    (conj (assoc base-row :status "FAIL"))
                                    (d/record-not-run-remaining rest-todo))
                             completed-count true nightly-failure?))
                  (do (d/eputs (str "::warning::xdbg@" short " nightly " label
                                    " probe eval failed; skipping"))
                      (recur rest-todo
                             (conj results (assoc base-row :status "SKIP-EVAL"))
                             completed-count required-failure? true))))

            ;; Defensive default — keeps the loop from crashing if a
            ;; future probe variant slips through unmapped.
            (do (d/eputs (str "::error::run-sequence: unexpected probe status "
                              status " for " short))
                (recur [] (-> results
                              (conj (assoc base-row :status "FAIL"))
                              (d/record-not-run-remaining rest-todo))
                       completed-count true nightly-failure?))))))))

(defn- default-sample-size [profile]
  (case profile "stable" 0 "nightly" 3))

(defn- bootstrap-and-pick!
  "Run git bootstrap-fetch then build the plan in-process."
  [sample-size]
  (let [{:keys [nightly-tags-fetched]} (git/bootstrap-fetch)]
    (binding [*out* *err*]
      (if (zero? nightly-tags-fetched)
        (println "::warning::no nightly tags matched; nightly sampling will be empty")
        (println (format "Fetching %d nightly tags" nightly-tags-fetched)))))
  (v/pick-versions {:sample-size sample-size}))

(defn cmd-run
  "Pick a plan + execute it. Opts: :profile, :sample-size. Trailing
   args after `--` are forwarded to every xdbg invocation."
  [{:keys [opts args]}]
  (let [{:keys [profile sample-size]} opts
        sample-size (or sample-size (default-sample-size profile))
        lenient? (= profile "nightly")
        xdbg-flags (str/join " " args)
        parsed (mapv d/parse-plan-entry (bootstrap-and-pick! sample-size))
        bad (some d/validate-entry parsed)]
    (when bad
      (d/eputs (str "cross-version-test: plan entry missing sha or role: "
                    (:entry bad)))
      (System/exit 2))
    (when (< (count parsed) 2)
      (d/eputs (str "cross-version-test: plan must have at least 2 entries; got "
                    (count parsed)))
      (System/exit 2))
    (d/emit-plan-step-summary! "cross-version" profile sample-size parsed)
    (d/write-plan-artifact! parsed)
    (let [out-dir (or (System/getenv "XVT_OUT_DIR")
                      (str (or (System/getenv "TMPDIR") "/tmp")
                           "/xvt-out-" (d/pid)))]
      (d/ensure-dir! out-dir)
      (when-let [gh-out (System/getenv "GITHUB_OUTPUT")]
        (spit gh-out (str "out_dir=" out-dir "\n") :append true))
      (let [env-extras (cond-> (d/setup-run-env! "xvt-")
                         (seq xdbg-flags) (assoc "XVT_XDBG_FLAGS" xdbg-flags))]
        (d/eputs (str "::notice::xvt XDBG_DB_ROOT=" (get env-extras "XVT_DB_ROOT")))
        (d/eputs (str "::notice::xvt output dir: " out-dir))
        (when (seq xdbg-flags)
          (d/eputs (str "::notice::xvt forwarding xdbg flags: " xdbg-flags)))
        (run-sequence-body env-extras parsed out-dir lenient?)))))

(def ^:private cli-table
  [{:cmds ["run"]
    :fn cmd-run
    :spec {:profile {:coerce :string
                     :default "stable"
                     :desc "stable | nightly"
                     :validate #{"stable" "nightly"}}
           :sample-size {:coerce :long
                         :desc "Number of nightly samples (default: 0 stable, 3 nightly)"
                         :validate nat-int?}}}
   {:cmds [] :fn (fn [_]
                   (println "Usage: cross-version-test run [--profile stable|nightly]")
                   (println "                              [--sample-size N] [-- xdbg-flags...]"))}])

(defn- cli-error-fn [{:keys [msg]}]
  (d/eputs (str "cross-version-test: " msg))
  (System/exit 2))

(defn -main [& args]
  (cli/dispatch cli-table args {:error-fn cli-error-fn}))

(when (= *file* (System/getProperty "babashka.file"))
  (apply -main *command-line-args*))
