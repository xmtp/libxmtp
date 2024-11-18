rm -f client1.db
rm -f client2.db

./examples/cli/xli.sh --db client1.db --testnet --env local register
output=$("./examples/cli/xli.sh" --db client1.db --testnet --env local info)
client_1_address=$(echo "$output" | grep 'addressess' | sed -E 's/.*\["([^"]*)"\].*/\1/')
echo $client_1_address

./examples/cli/xli.sh --db client2.db --testnet --env local register
output=$("./examples/cli/xli.sh" --db client2.db --testnet --env local info)
client_2_address=$(echo "$output" | grep 'addressess' | sed -E 's/.*\["([^"]*)"\].*/\1/')
echo $client_2_address

./examples/cli/xli.sh --db client1.db --testnet --env local create-group

output=$("./examples/cli/xli.sh" --db client1.db --testnet --env local list-groups)
group_id=$(echo "$output" | grep 'SerializableGroup' | sed -E 's/.*group_id: "([^"]*)".*/\1/' | head -n 1)

echo "$group_id"


./examples/cli/xli.sh --db client1.db --testnet --env local add-group-members "$group_id" -a "$client_2_address"

./examples/cli/xli.sh --db client1.db --testnet --env local send "$group_id" "Automated message send from client 1"

./examples/cli/xli.sh --db client2.db --testnet --env local send "$group_id" "Automated message send from client 2"

./examples/cli/xli.sh --db client1.db --testnet --env local list-group-messages "$group_id"

./examples/cli/xli.sh --db client2.db --testnet --env local list-group-messages "$group_id"