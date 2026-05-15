(ns xdbg-driver-lib.versions-test
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.string :as str]
            [xdbg-driver-lib.versions :as v]
            [xdbg-driver-lib.git :as git]))

(deftest parse-stable-branch-test
  (is (= {:name "v1.9" :major 1 :minor 9 :patch 0}
         (v/parse-stable-branch "v1.9")))
  (is (= {:name "1.10.0" :major 1 :minor 10 :patch 0}
         (v/parse-stable-branch "1.10.0")))
  (is (nil? (v/parse-stable-branch "not-a-version")))
  (is (nil? (v/parse-stable-branch "v1"))))

(deftest last-two-stable-branches-test
  (testing "dedupe by major.minor (newest patch wins), return newest first"
    (with-redefs [git/for-each-ref
                  (fn [_]
                    ["origin/release/v1.9"
                     "origin/release/1.10.0"
                     "origin/release/v1.9.1"
                     "origin/release/random-junk"])]
      (is (= ["1.10.0" "v1.9.1"]
             (v/last-two-stable-branches))))))

(deftest nightly-tag-candidates-test
  (testing "extract (date sha7) from tag lines, dedupe"
    (with-redefs [git/tags-by-creator-date
                  (fn []
                    ["node-bindings-1.10.0-nightly.20260512.abc1234"
                     "wasm-bindings-1.10.0-nightly.20260512.abc1234" ; dupe by (date sha7)
                     "android-1.10.0-nightly.20260511.def5678"
                     "node-bindings-1.10.0"])]
      (is (= [["20260512" "abc1234"] ["20260511" "def5678"]]
             (v/nightly-tag-candidates))))))

(deftest pick-versions-no-nightlies-test
  (testing "sample-size 0 returns the 3-version base set, oldest is creator"
    (with-redefs [v/last-two-stable-branches    (fn [] ["1.10.0" "v1.9"])
                  git/rev-parse-multi           (fn [& _] ["sha-1.10" "sha-1.9" "sha-head"])
                  git/abbrev-ref-head           (fn [] nil)]
      (let [plan (v/pick-versions {:sample-size 0})]
        (is (= 3 (count plan)))
        (is (= "creator" (:role (first plan))) "oldest is creator")
        (is (= "v1.9" (-> plan first :branch (str/replace "release/" ""))))
        (is (= "1.10.0" (-> plan second :branch (str/replace "release/" ""))))
        (is (= "HEAD" (-> plan last :label)))))))
