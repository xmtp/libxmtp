(ns xdbg-driver-lib.xdbg-test
  (:require [clojure.test :refer [deftest is testing use-fixtures]]
            [babashka.fs :as fs]
            [babashka.process :as p]
            [xdbg-driver-lib.xdbg :as xdbg]))

(defn with-tmp-cache [f]
  (let [tmp (str (fs/create-temp-file {:prefix "xdbg-cache-"}))]
    (binding [xdbg/*cache-file* tmp]
      (try (f) (finally (fs/delete-if-exists tmp))))))

(use-fixtures :each with-tmp-cache)

(deftest cache-roundtrip-test
  (testing "cache stores and retrieves entries by key"
    (xdbg/cache-store "k1" 0)
    (xdbg/cache-store "k2" 1)
    (is (= 0 (xdbg/cache-lookup "k1")))
    (is (= 1 (xdbg/cache-lookup "k2")))
    (is (nil? (xdbg/cache-lookup "k-missing")))))

(deftest supports-flag-cache-hit
  (testing "second call doesn't re-shell"
    (let [calls (atom 0)]
      (with-redefs [p/shell (fn [_ & _]
                              (swap! calls inc)
                              {:exit 0 :out "--my-flag" :err ""})]
        (is (true? (xdbg/supports-flag "head" "abc" "--my-flag")))
        (is (true? (xdbg/supports-flag "head" "abc" "--my-flag")))
        (is (= 1 @calls))))))

(deftest probe-modes-test
  (testing "probe dry mode adds --dry-run"
    (let [args (atom nil)]
      (with-redefs [p/shell (fn [_ & a]
                              (reset! args a)
                              {:exit 0 :out "" :err ""})]
        (xdbg/probe "dry" "stable" "abc")
        (is (some #{"--dry-run"} @args))))))

(deftest probe-classifies-build-failure
  (testing "exit 1 with builder-failed in stderr returns :build-failed"
    (with-redefs [p/shell (fn [_ & _]
                            {:exit 1 :out ""
                             :err "error: build of '/nix/store/...' failed"})]
      (let [{:keys [status stderr]} (xdbg/probe "full" "nightly" "abc")]
        (is (= :build-failed status))
        (is (re-find #"build of" stderr))))))

(deftest probe-classifies-eval-failure
  (testing "exit 1 without builder-failed returns :eval-failed"
    (with-redefs [p/shell (fn [_ & _]
                            {:exit 1 :out "" :err "error: attribute missing"})]
      (let [{:keys [status exit stderr]} (xdbg/probe "dry" "stable" "abc")]
        (is (= :eval-failed status))
        (is (= 1 exit))
        (is (re-find #"attribute missing" stderr))))))

