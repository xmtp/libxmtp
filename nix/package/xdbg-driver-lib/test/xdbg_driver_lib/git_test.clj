(ns xdbg-driver-lib.git-test
  (:require [clojure.test :refer [deftest is testing]]
            [xdbg-driver-lib.git :as git]
            [babashka.process :as p]))

(deftest rev-parse-verify-test
  (testing "rev-parse-verify returns nil on missing commit"
    (with-redefs [p/shell (fn [_ & _]
                            {:exit 128 :out "" :err "unknown revision"})]
      (is (nil? (git/rev-parse-verify "deadbeef^{commit}"))))))

(deftest filter-nightly-tags-test
  (testing "filter-nightly-tags keeps only nightly-suffixed tags from supported binding families"
    (let [input ["refs/tags/node-bindings-1.10.0-nightly.20260512.abc1234"
                 "refs/tags/wasm-bindings-1.10.0-nightly.20260511.def5678"
                 "refs/tags/android-1.10.0-nightly.20260510.aaaa111"
                 "refs/tags/ios-1.10.0-nightly.20260509.bbbb222"
                 "refs/tags/cli-1.0.0-nightly.20260512.cccc333"   ; cli is excluded by design
                 "refs/tags/v1.10.0"                              ; non-nightly
                 "refs/tags/node-bindings-1.10.0"                 ; non-nightly
                 ""]]
      (is (= #{"node-bindings-1.10.0-nightly.20260512.abc1234"
               "wasm-bindings-1.10.0-nightly.20260511.def5678"
               "android-1.10.0-nightly.20260510.aaaa111"
               "ios-1.10.0-nightly.20260509.bbbb222"}
             (set (git/filter-nightly-tags input)))))))

(deftest bootstrap-fetch-no-tags
  (testing "bootstrap-fetch with no nightly tags returns count 0, does branch fetch only"
    (let [calls (atom [])]
      (with-redefs
        [p/shell (fn [_ & args]
                   (swap! calls conj (vec args))
                   (cond
                     (some #{"ls-remote"} args)
                     {:exit 0 :out "" :err ""}
                     :else
                     {:exit 0 :out "" :err ""}))]
        (let [{:keys [branches-fetched nightly-tags-fetched]} (git/bootstrap-fetch)]
          (is (true? branches-fetched))
          (is (zero? nightly-tags-fetched)))
        (let [fetch-calls (filter #(some #{"fetch"} %) @calls)]
          (is (= 1 (count fetch-calls))
              "only one fetch (branches); no tags fetch because list was empty"))))))

(deftest bootstrap-fetch-with-tags
  (testing "bootstrap-fetch fetches branches and matched nightly tags, returns count"
    (let [calls (atom [])]
      (with-redefs
        [p/shell (fn [_ & args]
                   (swap! calls conj (vec args))
                   (cond
                     (some #{"ls-remote"} args)
                     {:exit 0
                      :out (str "hash1\trefs/tags/node-bindings-1.10.0-nightly.20260512.abc1234\n"
                                "hash2\trefs/tags/v1.10.0\n")
                      :err ""}
                     :else
                     {:exit 0 :out "" :err ""}))]
        (let [{:keys [nightly-tags-fetched]} (git/bootstrap-fetch)]
          (is (= 1 nightly-tags-fetched)))
        (let [fetch-calls (filter #(some #{"fetch"} %) @calls)
              all-args (apply concat fetch-calls)]
          (is (>= (count fetch-calls) 2)
              "two fetches: one for branches, one for the matched tag")
          (is (some #(when (string? %) (clojure.string/includes? % "refs/tags/node-bindings-1.10.0-nightly.20260512.abc1234"))
                    all-args)
              "matched nightly tag is included in fetch refspec")
          (is (not (some #(when (string? %) (clojure.string/includes? % "v1.10.0"))
                         all-args))
              "non-nightly tag is excluded"))))))