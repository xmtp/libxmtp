rm -f client1.db
rm -f client2.db

./examples/cli/xli.sh --db client1.db --testnet --local register
output=$("./examples/cli/xli.sh" --db client1.db --testnet --local info)
client_1_address=$(echo "$output" | grep 'addressess' | sed -E 's/.*\["([^"]*)"\].*/\1/')
echo $client_1_address

./examples/cli/xli.sh --db client2.db --testnet --local register
output=$("./examples/cli/xli.sh" --db client2.db --testnet --local info)
client_2_address=$(echo "$output" | grep 'addressess' | sed -E 's/.*\["([^"]*)"\].*/\1/')
echo $client_2_address

./examples/cli/xli.sh --db client1.db --testnet --local create-group

output=$("./examples/cli/xli.sh" --db client1.db --testnet --local list-groups)
group_id=$(echo "$output" | grep 'SerializableGroup' | sed -E 's/.*group_id: "([^"]*)".*/\1/' | head -n 1)

echo "$group_id"

./examples/cli/xli.sh --db client1.db --testnet --local send "$group_id" "Automated message send"

./examples/cli/xli.sh --db client1.db --testnet --local add-group-members "$group_id" -a "$client_2_address"