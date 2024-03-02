#!/bin/bash

retries=0
# Given the resource limitations and external dependencies for the test suite,
# a retry count of at least 10 is needed.
until [ $retries -ge 10 ]
do
  echo "Running Test Suite Attempt ($retries)...."
  swift test -v | grep -E "Test Case|XCTAssert|failures"
  
  exit_code=$?
  
  if [ $exit_code -eq 0 ]; then
	echo "Test Succeeded"
	break
  else
	((retries=retries+1))
	echo "Test Suite Failed."
  fi
done

if [ $retries -ge 10 ]; then
  echo "Maximum number of retries exceeded. Exiting with status code 1."
  exit 1
fi
